use super::*;
// P1's game_id is PREDICTED (read_next_game_id) before create_game lands.
    // A concurrent create can take that id: live 2026-08-14, a T1003 game stole
    // the e2e's predicted id, so game-api told P2 to join a game whose
    // tournament (1003) differed from its own (1099) — join_game failed with
    // ConstraintSeeds on `tournament`, and P1's later commit_guess hit
    // InvalidGameState on a forever-unjoined game. resolve_created_game_id is
    // the post-land referential-integrity check: OUR game is the one whose
    // player_one and tournament match, at or after the predicted id.
    mod foreign_tournament {
        use super::super::foreign_tournament;

        // Cross-session contention, 2026-08-14: a second e2e driver sharing
        // this wallet queued in T1099; its pairing arrived over the shared
        // per-wallet WS while THIS flow was mid-T1003. Adopting it created a
        // cross-tournament game whose P2 looped on ConstraintSeeds. A match
        // for a different tournament than the session's must never be adopted.
        #[test]
        fn rejects_a_match_from_a_different_tournament() {
            assert!(foreign_tournament(Some(1003), Some(1099)));
        }

        #[test]
        fn accepts_a_match_for_the_sessions_own_tournament() {
            assert!(!foreign_tournament(Some(1003), Some(1003)));
        }

        #[test]
        fn accepts_when_the_server_predates_the_field() {
            // Pre-rollout game-api omits tournament_id — cannot fail closed on
            // information that does not exist.
            assert!(!foreign_tournament(Some(1003), None));
        }

        #[test]
        fn accepts_when_the_session_has_not_bound_a_tournament_yet() {
            assert!(!foreign_tournament(None, Some(1099)));
        }
    }

    mod resolve_created_game_id {
        use super::super::resolve_created_game_id;
        use solana_sdk::pubkey::Pubkey;

        #[test]
        fn keeps_the_predicted_id_when_it_is_ours() {
            let me = Pubkey::new_unique();
            let probes = vec![(42u64, me, 1099u64)];
            assert_eq!(resolve_created_game_id(42, &probes, &me, 1099), Some(42));
        }

        #[test]
        fn corrects_forward_when_a_concurrent_create_stole_the_id() {
            // The exact live shape: predicted id belongs to someone else's
            // game in another tournament; ours landed one later.
            let me = Pubkey::new_unique();
            let thief = Pubkey::new_unique();
            let probes = vec![(42u64, thief, 1003u64), (43u64, me, 1099u64)];
            assert_eq!(resolve_created_game_id(42, &probes, &me, 1099), Some(43));
        }

        #[test]
        fn none_when_our_game_is_nowhere_in_the_window() {
            let me = Pubkey::new_unique();
            let thief = Pubkey::new_unique();
            let probes = vec![(42u64, thief, 1003u64)];
            assert_eq!(resolve_created_game_id(42, &probes, &me, 1099), None);
        }

        #[test]
        fn same_wallet_wrong_tournament_is_not_ours() {
            // Same wallet playing two tournaments concurrently must not
            // cross-adopt games.
            let me = Pubkey::new_unique();
            let probes = vec![(42u64, me, 1003u64), (44u64, me, 1099u64)];
            assert_eq!(resolve_created_game_id(42, &probes, &me, 1099), Some(44));
        }
    }

    // The agent auth path now hangs entirely on reading the nonce back out of
    // the transaction. It replaced an in-memory GameSession field that was lost
    // whenever a session rehydrated from Firestore on another instance, which
    // is why two-agent runs failed with "no auth nonce for this stake" while
    // single-agent runs passed.
    #[test]
    fn memo_nonce_round_trips_out_of_a_signed_tx() {
        use solana_sdk::{signature::Keypair, signer::Signer, transaction::Transaction};
        let payer = Keypair::new();
        let nonce = "7f3a9c1e-4b2d-4e8f-9a1c-2d3e4f5a6b7c";
        let ixs = vec![
            solana_sdk::system_instruction::transfer(&payer.pubkey(), &payer.pubkey(), 1),
            game_chain::instructions::build_memo(nonce),
        ];
        let tx = Transaction::new_signed_with_payer(
            &ixs,
            Some(&payer.pubkey()),
            &[&payer],
            solana_sdk::hash::Hash::default(),
        );
        let bytes = bincode::serialize(&tx).unwrap();
        assert_eq!(memo_nonce_from_signed_tx(&bytes).as_deref(), Some(nonce));
    }

    #[test]
    fn a_tx_without_a_memo_yields_no_nonce() {
        // Must be None, not a spurious match on another instruction's data —
        // game-api matches on the Memo PROGRAM ID, so anything else would
        // authenticate nothing and should fail loudly at submit instead.
        use solana_sdk::{signature::Keypair, signer::Signer, transaction::Transaction};
        let payer = Keypair::new();
        let tx = Transaction::new_signed_with_payer(
            &[solana_sdk::system_instruction::transfer(
                &payer.pubkey(),
                &payer.pubkey(),
                1,
            )],
            Some(&payer.pubkey()),
            &[&payer],
            solana_sdk::hash::Hash::default(),
        );
        let bytes = bincode::serialize(&tx).unwrap();
        assert_eq!(memo_nonce_from_signed_tx(&bytes), None);
    }

    #[test]
    fn garbage_bytes_yield_no_nonce_rather_than_panicking() {
        assert_eq!(memo_nonce_from_signed_tx(&[0xff, 0x00, 0x13]), None);
        assert_eq!(memo_nonce_from_signed_tx(&[]), None);
    }

    #[test]
    fn session_state_transitions() {
        // Verify the state enum values are distinct.
        assert_ne!(GameSessionState::Connected, GameSessionState::Queued);
        assert_ne!(GameSessionState::Queued, GameSessionState::Matched);
        assert_ne!(GameSessionState::InGame, GameSessionState::Resolved);
    }

    /// Regression: 2026-05-09 devnet gameplay E2E sweep. The MCP server's
    /// game tools used to broadcast every signed tx through the cached
    /// per-wallet GameTxBuilder, which was constructed from the cluster
    /// default RPC URL only. The human-vs-agent test ran against devnet
    /// (T1001), but the agent's deposit_stake landed on mainnet RPC where
    /// T1001 doesn't exist → AccountNotInitialized. The router now picks
    /// per-call so `Some("devnet")` ≠ `Some("mainnet")` ≠ default.
    #[test]
    fn pick_rpc_url_for_network_routes_per_arg() {
        let default = "https://default.example";
        let mainnet = "https://mainnet.example";
        let devnet = "https://devnet.example";
        assert_eq!(
            pick_rpc_url_for_network(Some("devnet"), default, mainnet, devnet),
            devnet
        );
        assert_eq!(
            pick_rpc_url_for_network(Some("mainnet"), default, mainnet, devnet),
            mainnet
        );
        assert_eq!(
            pick_rpc_url_for_network(None, default, mainnet, devnet),
            default
        );
        // Unknown network falls through to default — never silently picks
        // mainnet or devnet from an arbitrary string.
        assert_eq!(
            pick_rpc_url_for_network(Some("stagenet"), default, mainnet, devnet),
            default
        );
    }

    /// Regression: 2026-05-09 devnet human-vs-mcp-agent E2E. When the
    /// browser-human reveals first, the on-chain matchup_type flips
    /// from MATCHUP_TYPE_UNSET (255) to 0 or 1. If the MCP agent's
    /// reveal then unconditionally passes r_matchup, the program
    /// rejects with `RMatchupMismatch` (error 6032) and the game stays
    /// in Revealing forever — browser stuck waiting for outcome,
    /// 120s assertion times out. The browser's `submitRevealTx` has
    /// the same race-aware logic; the MCP server must match.
    #[test]
    fn pick_reveal_matchup_arg_passes_when_unset() {
        let r_matchup = [42u8; 32];
        assert_eq!(
            pick_reveal_matchup_arg(MATCHUP_TYPE_UNSET, r_matchup),
            Some(r_matchup),
        );
    }

    /// The 2026-05-09 fix above made the DECISION correct. It did not make the
    /// INPUT fresh, and the same failure came back in a different costume.
    ///
    /// `pick_reveal_matchup_arg` is only as good as the `matchup_type` handed
    /// to it. Reading the game at the default `confirmed` commitment hides an
    /// opponent's reveal that is already processed, so the server concludes it
    /// is the first revealer, attaches r_matchup, and the program rejects with
    /// RMatchupMismatch — on every retry, because each retry re-reads at the
    /// same stale commitment. Live cost: 17-18 minutes per homogeneous cell
    /// (both reveals near-simultaneous) versus 2.3 minutes for heterogeneous
    /// ones, where the reveals are naturally staggered.
    ///
    /// This is a SOURCE-level guard, and deliberately so: the defect is a
    /// commitment level passed to an RPC, which no pure test can observe. It is
    /// weaker than a behavioural test — it proves the call site, not the
    /// outcome — but it fails if someone restores the stale read, which is the
    /// regression that actually happened.
    #[test]
    fn reveal_tx_reads_the_game_at_the_freshest_commitment() {
        let src = include_str!("game_session.rs");
        let build_fn = src
            .split("async fn build_reveal_tx")
            .nth(1)
            .expect("build_reveal_tx must exist");
        // Bound the window to this function so an unrelated read elsewhere
        // cannot satisfy the assertion.
        let body = &build_fn[..build_fn.len().min(4000)];
        assert!(
            body.contains("read_game_freshest("),
            "the reveal path must read at the freshest commitment; a `confirmed` \
             read makes the second revealer attach r_matchup and be rejected \
             with RMatchupMismatch on every retry"
        );
    }

    /// The resolution-broadcast path reads back OUR OWN write, so it has the
    /// same commitment requirement as the reveal path — for a different reason.
    ///
    /// `after_reveal_guess` runs right after our reveal lands and returns early
    /// unless the game reads as Resolved. At `confirmed`, our own transaction
    /// can still be invisible, and the early return silently swallows the case
    /// where OUR reveal was the one that resolved the game: game-api never
    /// hears about it, the session stays unresolved, and the outcome is missing
    /// from collected data. Nothing errors, which is what makes it survive.
    ///
    /// Source-level for the same reason as the reveal guard: a commitment level
    /// handed to an RPC is not observable from a pure test.
    #[test]
    fn resolution_broadcast_reads_the_game_at_the_freshest_commitment() {
        let src = include_str!("game_session.rs");
        let f = src
            .split("async fn after_reveal_guess")
            .nth(1)
            .expect("after_reveal_guess must exist");
        let body = &f[..f.len().min(4000)];
        assert!(
            body.contains("read_game_freshest("),
            "after_reveal_guess must read at the freshest commitment; it reads back \
             our own reveal, and a `confirmed` read makes a resolution we caused \
             look like an opponent who has not revealed, so it is never broadcast"
        );
    }

    /// Every read that can race an agent's own transaction must use the
    /// freshest commitment, not just the reveal path.
    ///
    /// get_result is called right after a reveal to learn the outcome. At
    /// `confirmed` the account can read as absent for a slot or two, and the
    /// caller is told "game account not found" — a flat falsehood about a game
    /// that exists, and one that reads like data loss rather than lag. The
    /// cross-surface metamorphic cell failed exactly this way and passed on
    /// retry, which is the signature of a timing bug being papered over.
    #[test]
    fn get_result_reads_the_game_at_the_freshest_commitment() {
        let src = include_str!("game_session.rs");
        let f = src
            .split("pub async fn get_result")
            .nth(1)
            .expect("get_result must exist");
        let body = &f[..f.len().min(2000)];
        assert!(
            body.contains("read_game_freshest("),
            "get_result must read at the freshest commitment; a `confirmed` read \
             reports a just-resolved game as not found"
        );
    }

    #[test]
    fn pick_reveal_matchup_arg_omits_when_already_revealed_same_team() {
        let r_matchup = [42u8; 32];
        // matchup_type=0 means same team — opponent already revealed.
        assert_eq!(pick_reveal_matchup_arg(0, r_matchup), None);
    }

    #[test]
    fn pick_reveal_matchup_arg_omits_when_already_revealed_diff_team() {
        let r_matchup = [42u8; 32];
        // matchup_type=1 means different teams — opponent already revealed.
        assert_eq!(pick_reveal_matchup_arg(1, r_matchup), None);
    }

    #[test]
    fn pick_reveal_matchup_arg_unset_sentinel_is_255() {
        // Pin the on-chain sentinel so a future refactor can't silently
        // drift from `programs/coordination-game/src/state/game.rs`.
        assert_eq!(MATCHUP_TYPE_UNSET, 255);
    }

    #[test]
    fn match_status_serialization() {
        let status = MatchStatus {
            status: "queued".to_string(),
            game_id: None,
            role: None,
            unsigned_tx: None,
            action: None,
            matchmaker_signature: None,
            blockhash: None,
        };
        let json = serde_json::to_string(&status).expect("serialize");
        assert!(json.contains("queued"));
    }

    // --- GameSessionState serialization ---

    #[test]
    fn game_session_state_roundtrip() {
        let variants = [
            GameSessionState::Connected,
            GameSessionState::Queued,
            GameSessionState::Matched,
            GameSessionState::InGame,
            GameSessionState::Committed,
            GameSessionState::Resolved,
        ];
        for state in &variants {
            let s = state.as_str();
            let restored = GameSessionState::from_str(s)
                .unwrap_or_else(|| panic!("failed to parse '{s}' back to GameSessionState"));
            assert_eq!(*state, restored, "roundtrip failed for '{s}'");
        }
    }

    #[test]
    fn game_session_state_from_unknown_returns_none() {
        assert!(GameSessionState::from_str("nonexistent").is_none());
    }

    // --- Preimage hex roundtrip ---

    #[test]
    fn preimage_hex_roundtrip() {
        let original: [u8; 32] = [
            0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
            0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x1b, 0x1c,
        ];
        let hex_str = hex::encode(original);
        let decoded = hex::decode(&hex_str).expect("valid hex");
        let restored: [u8; 32] = decoded.try_into().expect("32 bytes");
        assert_eq!(original, restored);
    }

    // --- PersistedGameSession serialization ---

    #[test]
    fn persisted_session_json_roundtrip() {
        let doc = PersistedGameSession {
            wallet: "Abc123".to_string(),
            jwt: "jwt-token".to_string(),
            state: "committed".to_string(),
            game_id: Some(42),
            tournament_id: Some(1),
            session_id: Some("sess-1".to_string()),
            role: Some(0),
            matchup_commitment: Some("deadbeef".to_string()),
            commit_preimage_hex: Some(hex::encode([0xabu8; 32])),
            game_ready: Some(42),
            reveal_data: None,
            updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        };
        let json = serde_json::to_string(&doc).expect("serialize");
        let restored: PersistedGameSession = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.wallet, "Abc123");
        assert_eq!(restored.state, "committed");
        assert_eq!(restored.game_id, Some(42));
        assert_eq!(restored.commit_preimage_hex, doc.commit_preimage_hex);
    }

    #[test]
    fn restore_skips_resolved_sessions() {
        let state = GameSessionState::from_str("resolved");
        assert_eq!(state, Some(GameSessionState::Resolved));
        // In register_wallet, resolved sessions are cleaned up, not restored.
        // This test verifies the state parsing works correctly.
        assert_eq!(state.unwrap(), GameSessionState::Resolved);
    }

    /// A session restored on a DIFFERENT instance must still know a match was
    /// found, or it can never advance past `queued`.
    ///
    /// mcp-server runs on Cloud Run with `sessionAffinity: false` and
    /// `maxScale: 30`, so consecutive `game_check_match` polls from one agent
    /// routinely land on different instances. `match_found` is only ever set by
    /// the in-process WS listener (`ws: match_found`), so an instance that
    /// restored the session from Firestore used to rebuild it with
    /// `match_found: None` — and `check_match` returns `queued` on exactly that
    /// condition. The agent then polled `queued` for the full 90s window, was
    /// never handed a create_game tx, and game-api abandoned the paired session
    /// with "game not created within timeout". Observed live twice in a row:
    /// player_one CKsZ7Z… (role 0) paired with the grok pool wallet, both
    /// sessions abandoned, `0 submit failure(s)` because no tx was ever issued.
    ///
    /// The three fields `MatchFoundMsg` needs are already persisted, so the
    /// restore can reconstruct it rather than dropping the signal.
    #[test]
    fn restore_rehydrates_match_found_so_another_instance_can_advance() {
        let persisted = PersistedGameSession {
            wallet: "CKsZ7Z".to_string(),
            jwt: "jwt".to_string(),
            state: "matched".to_string(),
            game_id: None,
            tournament_id: Some(1003),
            session_id: Some("d058141e".to_string()),
            role: Some(0),
            matchup_commitment: Some("deadbeef".to_string()),
            commit_preimage_hex: None,
            game_ready: None,
            reveal_data: None,
            updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        };

        let s = build_restored_session("CKsZ7Z", &persisted, GameSessionState::Matched);

        let mf = s
            .match_found
            .expect("a restored matched session must carry match_found, else check_match returns queued forever");
        assert_eq!(mf.session_id, "d058141e");
        assert_eq!(mf.role, 0);
        assert_eq!(mf.matchup_commitment.as_deref(), Some("deadbeef"));
    }

    /// The reader-side half of the cross-instance fix. Write-through alone is
    /// not enough: an instance that restored the session BEFORE the match landed
    /// holds `match_found: None` in memory forever, and `check_match` reads that
    /// map rather than Firestore. Live evidence that this half was missing — the
    /// e2e passed only on RETRY, attempt 1 still dying with
    /// "no match within the window (90s): last status=queued".
    #[test]
    fn stale_instance_adopts_a_match_recorded_elsewhere() {
        let mut s = build_restored_session(
            "CKsZ7Z",
            &PersistedGameSession {
                wallet: "CKsZ7Z".to_string(),
                jwt: "jwt".to_string(),
                state: "queued".to_string(),
                game_id: None,
                tournament_id: Some(1003),
                session_id: None,
                role: None,
                matchup_commitment: None,
                commit_preimage_hex: None,
                game_ready: None,
                reveal_data: None,
                updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
            },
            GameSessionState::Queued,
        );
        assert!(
            s.match_found.is_none(),
            "precondition: stale, pre-match copy"
        );

        // Meanwhile the owning instance's WS listener persisted the match.
        let persisted_by_other_instance = PersistedGameSession {
            wallet: "CKsZ7Z".to_string(),
            jwt: "jwt".to_string(),
            state: "matched".to_string(),
            game_id: None,
            tournament_id: Some(1003),
            session_id: Some("d058141e".to_string()),
            role: Some(0),
            matchup_commitment: Some("deadbeef".to_string()),
            commit_preimage_hex: None,
            game_ready: None,
            reveal_data: None,
            updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        };

        assert!(
            adopt_persisted_match(&mut s, &persisted_by_other_instance),
            "a stale instance must adopt the match instead of answering queued"
        );
        assert_eq!(s.role, Some(0));
        assert_eq!(s.session_id.as_deref(), Some("d058141e"));
        assert_eq!(s.match_found.expect("adopted").session_id, "d058141e");
    }

    /// Adoption must not fabricate a match, and must not clobber a live one.
    #[test]
    fn adopt_is_a_no_op_without_a_recorded_match_or_when_already_matched() {
        let base = PersistedGameSession {
            wallet: "CKsZ7Z".to_string(),
            jwt: "jwt".to_string(),
            state: "queued".to_string(),
            game_id: None,
            tournament_id: Some(1003),
            session_id: None,
            role: None,
            matchup_commitment: None,
            commit_preimage_hex: None,
            game_ready: None,
            reveal_data: None,
            updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        };

        // Nothing recorded upstream -> no adoption.
        let mut s = build_restored_session("CKsZ7Z", &base, GameSessionState::Queued);
        assert!(!adopt_persisted_match(&mut s, &base));
        assert!(s.match_found.is_none());

        // Already matched in memory -> the WS-delivered copy wins.
        let matched = PersistedGameSession {
            session_id: Some("stale-doc".to_string()),
            role: Some(1),
            ..base.clone()
        };
        let mut live = build_restored_session(
            "CKsZ7Z",
            &PersistedGameSession {
                session_id: Some("live-ws".to_string()),
                role: Some(0),
                ..base.clone()
            },
            GameSessionState::Matched,
        );
        assert!(live.match_found.is_some(), "precondition: already matched");
        assert!(!adopt_persisted_match(&mut live, &matched));
        assert_eq!(
            live.match_found.expect("kept").session_id,
            "live-ws",
            "must not clobber the live WS-delivered match"
        );
    }

    /// Helper: a stale, pre-match in-memory session (the state an instance is
    /// left in when it restored before the match landed).
    fn stale_queued_session() -> GameSession {
        build_restored_session("CKsZ7Z", &unmatched_doc(), GameSessionState::Queued)
    }

    fn unmatched_doc() -> PersistedGameSession {
        PersistedGameSession {
            wallet: "CKsZ7Z".to_string(),
            jwt: "jwt".to_string(),
            state: "queued".to_string(),
            game_id: None,
            tournament_id: Some(1003),
            session_id: None,
            role: None,
            matchup_commitment: None,
            commit_preimage_hex: None,
            game_ready: None,
            reveal_data: None,
            updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        }
    }

    fn matched_doc() -> PersistedGameSession {
        PersistedGameSession {
            state: "matched".to_string(),
            session_id: Some("d058141e".to_string()),
            role: Some(0),
            matchup_commitment: Some("deadbeef".to_string()),
            ..unmatched_doc()
        }
    }

    /// REGRESSION GUARD. Rehydrating `match_found` from any doc that merely has
    /// a session_id + role adopts a match that was already CONSUMED by a
    /// previous game, and the agent then "creates game as P1" against a session
    /// that has ended.
    ///
    /// Observed live: restore reported game 731 from the prior run, the agent
    /// joined the queue at :14 and created game 732 at :17 — three seconds
    /// later, before any match existed — and the REAL `ws: match_found` only
    /// arrived at :53, 34s after the game had been created. commit_guess then
    /// hit 0x1770 because the game it created had no opponent.
    ///
    /// Only `Matched` means "a match is pending and unprocessed". InGame,
    /// Committed and Resolved all mean the match was already acted on.
    #[test]
    fn restore_does_not_rehydrate_a_match_already_consumed_by_a_previous_game() {
        // Resolved is excluded here only because `build_restored_session`
        // debug_asserts its caller already filtered those out; the adopt path
        // below still rejects a resolved doc.
        for consumed in [GameSessionState::InGame, GameSessionState::Committed] {
            let persisted = PersistedGameSession {
                state: consumed.as_str().to_string(),
                game_id: Some(731),
                ..matched_doc()
            };
            let s = build_restored_session("CKsZ7Z", &persisted, consumed);
            assert!(
                s.match_found.is_none(),
                "state {:?} means the match was already consumed — rehydrating it \
                 makes the agent create a game against a finished session",
                consumed
            );
        }

        // The one state that SHOULD rehydrate: a match found but not yet acted on.
        let s = build_restored_session("CKsZ7Z", &matched_doc(), GameSessionState::Matched);
        assert!(
            s.match_found.is_some(),
            "Matched is exactly the pending-and-unprocessed case the cross-instance fix needs"
        );
    }

    /// Same guard on the reader-side path.
    #[test]
    fn adopt_refuses_a_persisted_match_that_was_already_consumed() {
        let mut s = stale_queued_session();
        let consumed = PersistedGameSession {
            state: "in_game".to_string(),
            game_id: Some(731),
            ..matched_doc()
        };
        assert_eq!(
            reconcile_queued_session(&mut s, Ok(Some(consumed))),
            ReconcileOutcome::StillQueued,
            "an in_game doc is a finished match; adopting it strands the agent on a dead session"
        );
        assert!(s.match_found.is_none());
    }

    /// HAPPY PATH: the owning instance persisted the match; this instance
    /// adopts it instead of answering `queued`.
    #[test]
    fn reconcile_adopts_a_match_persisted_by_the_owning_instance() {
        let mut s = stale_queued_session();
        let outcome = reconcile_queued_session(&mut s, Ok(Some(matched_doc())));
        assert_eq!(outcome, ReconcileOutcome::Adopted);
        assert_eq!(s.role, Some(0));
        assert_eq!(s.match_found.expect("adopted").session_id, "d058141e");
    }

    /// ERROR PATH 1 — the Firestore read FAILS. The poll must not error out
    /// (the agent keeps polling), but it must report queued rather than
    /// pretending a match exists.
    #[test]
    fn reconcile_reports_queued_when_the_firestore_read_fails() {
        let mut s = stale_queued_session();
        let outcome =
            reconcile_queued_session(&mut s, Err(anyhow::anyhow!("firestore unavailable")));
        assert_eq!(outcome, ReconcileOutcome::StillQueued);
        assert!(
            s.match_found.is_none(),
            "a failed read must never fabricate a match"
        );
    }

    /// ERROR PATH 2 — the document is absent, or present but records no match.
    /// Both mean genuinely still queued.
    #[test]
    fn reconcile_reports_queued_when_nothing_upstream_recorded_a_match() {
        let mut s = stale_queued_session();
        assert_eq!(
            reconcile_queued_session(&mut s, Ok(None)),
            ReconcileOutcome::StillQueued
        );
        assert!(s.match_found.is_none());

        let mut s2 = stale_queued_session();
        assert_eq!(
            reconcile_queued_session(&mut s2, Ok(Some(unmatched_doc()))),
            ReconcileOutcome::StillQueued
        );
        assert!(s2.match_found.is_none());
    }

    /// The producer/reader CONTRACT. The WS write-through and this reader are
    /// two halves of one fix: whatever `build_persisted_doc` stores when the WS
    /// listener sees `match_found` must be exactly what the reader needs to
    /// adopt. If a future change drops session_id or role from the doc, the
    /// stall returns silently — this test fails instead.
    #[test]
    fn persisted_ws_transition_carries_everything_the_reader_needs_to_adopt() {
        // The state the WS handler leaves the session in on `ws: match_found`.
        let mut owner = stale_queued_session();
        owner.session_id = Some("d058141e".to_string());
        owner.role = Some(0);
        owner.matchup_commitment = Some("deadbeef".to_string());
        owner.match_found = Some(MatchFoundMsg {
            session_id: "d058141e".to_string(),
            role: 0,
            matchup_commitment: Some("deadbeef".to_string()),
            tournament_id: Some(1),
        });

        let doc = build_persisted_doc(&owner);

        // A different instance reads that doc and must be able to adopt.
        let mut other = stale_queued_session();
        assert_eq!(
            reconcile_queued_session(&mut other, Ok(Some(doc))),
            ReconcileOutcome::Adopted,
            "the doc written on ws:match_found must be sufficient for another instance to advance"
        );
        assert_eq!(other.match_found.expect("adopted").role, 0);
    }

    /// The inverse: a genuinely un-matched session must NOT be rehydrated into
    /// a bogus match, or `check_match` would try to advance a player who was
    /// never paired.
    #[test]
    fn restore_leaves_match_found_empty_when_no_match_was_recorded() {
        let persisted = PersistedGameSession {
            wallet: "CKsZ7Z".to_string(),
            jwt: "jwt".to_string(),
            state: "queued".to_string(),
            game_id: None,
            tournament_id: Some(1003),
            session_id: None,
            role: None,
            matchup_commitment: None,
            commit_preimage_hex: None,
            game_ready: None,
            reveal_data: None,
            updated_at: firestore::FirestoreTimestamp(chrono::Utc::now()),
        };

        let s = build_restored_session("CKsZ7Z", &persisted, GameSessionState::Queued);
        assert!(
            s.match_found.is_none(),
            "no session_id/role persisted means no match was ever found"
        );
    }

    #[test]
    fn restore_recovers_committed_session_with_preimage() {
        let preimage_bytes = [0x42u8; 32];
        let hex_str = hex::encode(preimage_bytes);

        // Simulate restoring from Firestore.
        let restored_preimage = hex::decode(&hex_str)
            .ok()
            .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok());

        assert_eq!(restored_preimage, Some(preimage_bytes));
    }

    // --- Cross-chain submit wiring (5d-B) ---

    #[test]
    fn xchain_solana_actions_are_recognized() {
        for a in [
            "create_xmatch",
            "lock_xtranche",
            "refund_xmatch_nocert",
            "refund_xmatch_timeout",
            "settle_xmatch",
        ] {
            assert!(is_xchain_solana_action(a), "{a} must be recognized");
        }
        // Same-chain actions and the EVM-leg (submitted off-server) are NOT.
        for a in [
            "create_game",
            "join_game",
            "commit_guess",
            "reveal_guess",
            "create_match",
        ] {
            assert!(!is_xchain_solana_action(a), "{a} must not be xchain");
        }
    }

    #[test]
    fn xchain_submit_result_routes_next_step_by_action() {
        let fund = xchain_submit_result("create_xmatch", "sig1");
        assert_eq!(fund["tx_signature"], "sig1");
        assert_eq!(fund["status"], "submitted");
        // After funding, the player locks their own leg (permissionless) — the
        // operator no longer locks.
        assert!(fund["next_step"]
            .as_str()
            .expect("next_step")
            .contains("xchain_build_lock_xmatch"));

        let lock = xchain_submit_result("lock_xtranche", "siglock");
        assert!(lock["next_step"]
            .as_str()
            .expect("next_step")
            .contains("locked"));

        let settle = xchain_submit_result("settle_xmatch", "sig2");
        assert!(settle["next_step"]
            .as_str()
            .expect("next_step")
            .contains("PlayerProfile"));

        // Both refund kinds fall through to the refund guidance.
        for kind in ["refund_xmatch_nocert", "refund_xmatch_timeout"] {
            let r = xchain_submit_result(kind, "sig3");
            assert!(r["next_step"]
                .as_str()
                .expect("next_step")
                .contains("Refund submitted"));
        }
    }
