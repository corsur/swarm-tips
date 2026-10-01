use super::*;
    use base64::Engine as _;

    fn encode_event(name: &str, body: &[u8]) -> String {
        let mut data = event_discriminator(name).to_vec();
        data.extend_from_slice(body);
        format!(
            "Program data: {}",
            base64::engine::general_purpose::STANDARD.encode(data)
        )
    }

    #[test]
    fn decodes_task_finalized_event_from_logs() {
        let mut body = 42u64.to_le_bytes().to_vec(); // task_id
        body.extend_from_slice(&[7u8; 32]); // agent pubkey
        body.extend_from_slice(&5000u64.to_le_bytes()); // payment
        body.extend_from_slice(&50u64.to_le_bytes()); // fee
        let logs = vec![
            "Program log: something".to_string(),
            encode_event("TaskFinalized", &body),
        ];
        let events = events_from_logs(&logs);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChainEvent::Finalized { task_id, agent } => {
                assert_eq!(*task_id, 42);
                assert_eq!(*agent, bs58::encode([7u8; 32]).into_string());
            }
            _ => panic!("wrong event kind"),
        }
    }

    #[test]
    fn decodes_created_verified_chain() {
        let mut created = 7u64.to_le_bytes().to_vec();
        created.extend_from_slice(&[9u8; 32]); // client
        created.extend_from_slice(&[0u8; 33]); // escrow+deadline+... (trailing ignored)
        let mut verified = 7u64.to_le_bytes().to_vec();
        verified.extend_from_slice(&800_000u64.to_le_bytes()); // composite_score
        verified.extend_from_slice(&[0u8; 48]);
        let logs = vec![
            encode_event("TaskCreated", &created),
            encode_event("TaskVerified", &verified),
        ];
        let events = events_from_logs(&logs);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], ChainEvent::Created { task_id: 7, .. }));
        match &events[1] {
            ChainEvent::Verified {
                task_id,
                composite_score,
            } => {
                assert_eq!(*task_id, 7);
                assert_eq!(*composite_score, 800_000);
            }
            _ => panic!("wrong event kind"),
        }
    }

    #[test]
    fn unknown_discriminators_are_ignored() {
        let logs = vec![
            encode_event("SomeOtherEvent", &[1, 2, 3]),
            "Program log: noise".to_string(),
        ];
        assert!(events_from_logs(&logs).is_empty());
    }

    #[test]
    fn challenge_resolved_decodes_challenger_won_flag() {
        let mut body = 3u64.to_le_bytes().to_vec();
        body.push(1); // challenger_won = true
        body.extend_from_slice(&100u64.to_le_bytes());
        let logs = vec![encode_event("ChallengeResolved", &body)];
        let events = events_from_logs(&logs);
        match &events[0] {
            ChainEvent::ChallengeResolved {
                task_id,
                challenger_won,
            } => {
                assert_eq!(*task_id, 3);
                assert!(*challenger_won);
            }
            _ => panic!("wrong event kind"),
        }
    }

    // ---- combinatorial log-sequence matrix -------------------------------
    //
    // The decoder must extract exactly the valid events from any interleaving
    // of valid frames, foreign frames, truncated frames, and non-event noise
    // — silently skipping garbage, never panicking, never inventing events.

    fn created_frame(task_id: u64) -> String {
        let mut body = task_id.to_le_bytes().to_vec();
        body.extend_from_slice(&[9u8; 32]);
        body.extend_from_slice(&[0u8; 33]);
        encode_event("TaskCreated", &body)
    }

    fn finalized_frame(task_id: u64) -> String {
        let mut body = task_id.to_le_bytes().to_vec();
        body.extend_from_slice(&[7u8; 32]);
        body.extend_from_slice(&[0u8; 16]);
        encode_event("TaskFinalized", &body)
    }

    fn noise_variants() -> Vec<(&'static str, String)> {
        vec![
            ("plain-log", "Program log: hello".to_string()),
            ("foreign-event", encode_event("SomeOtherEvent", &[1, 2, 3])),
            (
                "truncated-frame",
                encode_event("TaskFinalized", &[1, 2, 3]), // far too short
            ),
            ("non-base64", "Program data: !!!not-base64!!!".to_string()),
            (
                "short-discriminator",
                format!(
                    "Program data: {}",
                    base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3])
                ),
            ),
        ]
    }

    #[test]
    fn every_noise_interleaving_extracts_exactly_the_valid_events() {
        // Two valid frames × every noise variant × every insertion slot
        // (before / between / after) — the extraction must be identical.
        for (noise_name, noise) in noise_variants() {
            for slot in 0..3 {
                let mut logs = vec![created_frame(1), finalized_frame(1)];
                logs.insert(slot, noise.clone());
                let events = events_from_logs(&logs);
                assert_eq!(
                    events.len(),
                    2,
                    "noise {noise_name:?} at slot {slot}: expected 2 events, got {}",
                    events.len()
                );
                assert!(
                    matches!(events[0], ChainEvent::Created { task_id: 1, .. }),
                    "noise {noise_name:?} at slot {slot}: first event wrong"
                );
                assert!(
                    matches!(events[1], ChainEvent::Finalized { task_id: 1, .. }),
                    "noise {noise_name:?} at slot {slot}: second event wrong"
                );
            }
        }
    }

    #[test]
    fn interleaved_tasks_keep_their_ids_straight() {
        // Frames from two tasks interleaved in one transaction's logs: the
        // decoder is order-preserving and never cross-wires task ids.
        let logs = vec![
            created_frame(10),
            created_frame(20),
            finalized_frame(20),
            finalized_frame(10),
        ];
        let events = events_from_logs(&logs);
        assert_eq!(events.len(), 4);
        let ids: Vec<u64> = events
            .iter()
            .map(|e| match e {
                ChainEvent::Created { task_id, .. } => *task_id,
                ChainEvent::Finalized { task_id, .. } => *task_id,
                other => panic!("unexpected event {other:?}"),
            })
            .collect();
        assert_eq!(ids, vec![10, 20, 20, 10]);
    }

    #[test]
    fn all_noise_no_signal_yields_nothing() {
        let logs: Vec<String> = noise_variants().into_iter().map(|(_, n)| n).collect();
        assert!(events_from_logs(&logs).is_empty());
    }

    // -----------------------------------------------------------------------
    // apply_events — the join-and-mint rules. These were ~65 lines inlined in
    // the middle of backfill, so the endpoint's whole reason for existing had
    // no coverage; only the log-decoding helpers were tested.
    // -----------------------------------------------------------------------

    fn at() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp")
    }

    #[test]
    fn a_full_lifecycle_mints_one_finalize_edge_with_the_scaled_weight() {
        let mut st = JoinState::default();
        let (docs, delta) = apply_events(
            vec![
                ChainEvent::Created {
                    task_id: 1,
                    client: "CLIENT".into(),
                },
                ChainEvent::Claimed {
                    task_id: 1,
                    agent: "AGENT".into(),
                },
                ChainEvent::Verified {
                    task_id: 1,
                    composite_score: MAX_SCORE / 4,
                },
                ChainEvent::Finalized {
                    task_id: 1,
                    agent: "AGENT".into(),
                },
            ],
            "sig-1",
            Some(at()),
            &mut st,
        );

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].from, "CLIENT");
        assert_eq!(docs[0].to, "AGENT");
        assert!((docs[0].weight - 0.25).abs() < f64::EPSILON);
        assert_eq!(docs[0].source, "shillbot_finalize");
        assert_eq!(docs[0].tx_sig, "sig-1");
        assert_eq!(
            delta,
            EventDelta {
                finalized: 1,
                challenge: 0,
                skipped_incomplete: 0
            }
        );
    }

    #[test]
    fn a_score_above_max_is_clamped_rather_than_minting_more_than_full_trust() {
        let mut st = JoinState::default();
        st.client_by_task.insert(1, "CLIENT".into());
        let (docs, _) = apply_events(
            vec![
                ChainEvent::Verified {
                    task_id: 1,
                    composite_score: MAX_SCORE * 5,
                },
                ChainEvent::Finalized {
                    task_id: 1,
                    agent: "AGENT".into(),
                },
            ],
            "sig",
            Some(at()),
            &mut st,
        );
        assert!((docs[0].weight - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_finalize_without_a_score_is_a_real_settlement_carrying_zero_trust() {
        let mut st = JoinState::default();
        st.client_by_task.insert(7, "CLIENT".into());
        let (docs, delta) = apply_events(
            vec![ChainEvent::Finalized {
                task_id: 7,
                agent: "AGENT".into(),
            }],
            "sig",
            Some(at()),
            &mut st,
        );
        // An edge IS drawn — the settlement happened — but it carries no trust.
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].weight, 0.0);
        assert_eq!(delta.finalized, 1);
        assert_eq!(delta.skipped_incomplete, 0);
    }

    #[test]
    fn a_finalize_with_no_known_client_is_skipped_not_invented() {
        let mut st = JoinState::default();
        let (docs, delta) = apply_events(
            vec![ChainEvent::Finalized {
                task_id: 9,
                agent: "AGENT".into(),
            }],
            "sig",
            Some(at()),
            &mut st,
        );
        assert!(docs.is_empty(), "no client means there is no edge to draw");
        assert_eq!(delta.skipped_incomplete, 1);
    }

    #[test]
    fn a_challenge_the_agent_won_mints_nothing() {
        let mut st = JoinState::default();
        st.client_by_task.insert(1, "CLIENT".into());
        st.agent_by_task.insert(1, "AGENT".into());
        let (docs, delta) = apply_events(
            vec![ChainEvent::ChallengeResolved {
                task_id: 1,
                challenger_won: false,
            }],
            "sig",
            Some(at()),
            &mut st,
        );
        // The agent won, so the Finalized path pays out and draws the edge.
        assert!(docs.is_empty());
        assert_eq!(delta, EventDelta::default());
    }

    #[test]
    fn a_challenge_the_challenger_won_mints_a_zero_weight_edge() {
        let mut st = JoinState::default();
        st.client_by_task.insert(1, "CLIENT".into());
        st.agent_by_task.insert(1, "AGENT".into());
        // Even a high score must not leak into a lost challenge.
        st.score_by_task.insert(1, MAX_SCORE);
        let (docs, delta) = apply_events(
            vec![ChainEvent::ChallengeResolved {
                task_id: 1,
                challenger_won: true,
            }],
            "sig",
            Some(at()),
            &mut st,
        );
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].source, "challenge_resolved");
        assert_eq!(docs[0].weight, 0.0);
        assert_eq!(delta.challenge, 1);
    }

    #[test]
    fn join_state_carries_across_signatures() {
        // Created and Finalized normally arrive in DIFFERENT transactions —
        // that is the entire reason JoinState exists.
        let mut st = JoinState::default();
        let (docs, _) = apply_events(
            vec![ChainEvent::Created {
                task_id: 3,
                client: "CLIENT".into(),
            }],
            "sig-a",
            Some(at()),
            &mut st,
        );
        assert!(docs.is_empty());

        let (docs, delta) = apply_events(
            vec![ChainEvent::Finalized {
                task_id: 3,
                agent: "AGENT".into(),
            }],
            "sig-b",
            Some(at()),
            &mut st,
        );
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].from, "CLIENT", "client carried over from sig-a");
        assert_eq!(docs[0].tx_sig, "sig-b", "edge id is the FINALIZE signature");
        assert_eq!(delta.finalized, 1);
    }
