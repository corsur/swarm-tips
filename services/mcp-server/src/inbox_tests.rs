use super::*;

// An arbitrary NON-support Solana wallet (wrapped-SOL mint pubkey — a
// well-known valid 32-byte base58 that is neither support wallet). It was
// formerly the treasury/root address, but that address is now a recognized
// support recipient (`SUPPORT_WALLET_ROOT`), so the generic "some other
// wallet" fixture must point elsewhere.
const SOL_B58: &str = "So11111111111111111111111111111111111111112";
const EVM_ADDR: &str = "0x996213ed4099707059b8b5d7489fff23dac9770d";
/// The root/treasury wallet — a SECOND support recipient (RECIPIENT-only).
const ROOT_B58: &str = "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";

fn ts(s: &str) -> FirestoreTimestamp {
    FirestoreTimestamp(
        chrono::DateTime::parse_from_rfc3339(s)
            .expect("valid rfc3339")
            .with_timezone(&chrono::Utc),
    )
}

// -- serde round-trips (schema drift fails here, not in prod) ----------

#[test]
fn message_doc_roundtrip() {
    let doc = InboxMessageDoc {
        schema: MESSAGE_SCHEMA.to_string(),
        msg_id: "00001756000000000000_0a1b2c3d".to_string(),
        from_wallet: format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2),
        to_wallet: format!("eip155:84532:{EVM_ADDR}"),
        thread_id: "task:abc:123".to_string(),
        intent: Some("task_clarification".to_string()),
        body: "when is the deadline?".to_string(),
        sent_at: ts("2026-08-24T00:00:00Z"),
        seed: true,
        direction: DIRECTION_SENT.to_string(),
    };
    let json = serde_json::to_string(&doc).expect("serialize");
    let parsed: InboxMessageDoc = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.schema, "swarm/v1");
    assert_eq!(parsed.msg_id, doc.msg_id);
    assert_eq!(parsed.intent.as_deref(), Some("task_clarification"));
    assert!(parsed.seed);
    assert_eq!(parsed.direction, "sent");
}

#[test]
fn message_doc_without_direction_defaults_to_received() {
    // Every pre-outbox doc in Firestore lacks the field — it must
    // deserialize as the recipient copy, not fail or come back "sent".
    let legacy = r#"{
            "schema":"swarm/v1","msg_id":"m1","from_wallet":"a","to_wallet":"b",
            "thread_id":"dm:a|b","intent":null,"body":"hi",
            "sent_at":"2026-08-24T00:00:00Z","expires_at":"2026-09-23T00:00:00Z",
            "seed":false
        }"#;
    let parsed: InboxMessageDoc = serde_json::from_str(legacy).expect("legacy deserializes");
    assert_eq!(parsed.direction, DIRECTION_RECEIVED);
}

#[test]
fn mailbox_meta_roundtrip_and_defaults() {
    let doc = MailboxMetaDoc {
        wallet: SOL_B58.to_string(),
        unread_count: 3,
        latest_cursor: "00000000000000000009_ffffffff".to_string(),
        read_watermark: "00000000000000000005_00000000".to_string(),
        updated_at: ts("2026-08-24T00:00:00Z"),
    };
    let json = serde_json::to_string(&doc).expect("serialize");
    let parsed: MailboxMetaDoc = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.unread_count, 3);
    assert_eq!(parsed.latest_cursor, doc.latest_cursor);

    // Partial doc (created by an ack before any send): counters default.
    let partial = r#"{"wallet":"w","read_watermark":"x","unread_count":0,
                          "updated_at":"2026-08-24T00:00:00Z"}"#;
    let parsed: MailboxMetaDoc = serde_json::from_str(partial).expect("partial deserializes");
    assert_eq!(parsed.latest_cursor, "", "missing cursor defaults empty");
}

#[test]
fn thread_meta_roundtrip_and_defaults() {
    let doc = ThreadMetaDoc {
        thread_id: "dm:a|b".to_string(),
        message_count: 42,
        muted: true,
        reported: true,
        last_msg_at: Some(ts("2026-08-24T00:00:00Z")),
        expires_at: Some(ts("2026-09-23T00:00:00Z")),
    };
    let json = serde_json::to_string(&doc).expect("serialize");
    let parsed: ThreadMetaDoc = serde_json::from_str(&json).expect("deserialize");
    assert!(parsed.muted && parsed.reported);
    assert_eq!(parsed.message_count, 42);

    // A mute-created doc has no timestamps or count.
    let partial = r#"{"thread_id":"t","muted":true,"reported":false}"#;
    let parsed: ThreadMetaDoc = serde_json::from_str(partial).expect("partial deserializes");
    assert_eq!(parsed.message_count, 0);
    assert!(parsed.last_msg_at.is_none());
}

#[test]
fn quota_doc_roundtrip_and_defaults() {
    let doc = QuotaDoc {
        wallet: SOL_B58.to_string(),
        date: "20260824".to_string(),
        sends: 4,
        reads: 100,
        posts: 7,
        expires_at: ts("2026-08-27T00:00:00Z"),
    };
    let json = serde_json::to_string(&doc).expect("serialize");
    let parsed: QuotaDoc = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.sends, 4);
    assert_eq!(parsed.reads, 100);
    assert_eq!(parsed.posts, 7);

    // Shell doc before any increment lands: counters default 0 —
    // including `posts` on quota docs written before W3.
    let shell = r#"{"wallet":"w","date":"20260824","expires_at":"2026-08-27T00:00:00Z"}"#;
    let parsed: QuotaDoc = serde_json::from_str(shell).expect("shell deserializes");
    assert_eq!((parsed.sends, parsed.reads, parsed.posts), (0, 0, 0));
}

#[test]
fn wallet_verification_roundtrip() {
    let doc = WalletVerificationDoc {
        wallet: format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2),
        method: "memo_tx".to_string(),
        proof_sig: "5sigsig".to_string(),
        first_verified_at: ts("2026-08-24T00:00:00Z"),
    };
    let json = serde_json::to_string(&doc).expect("serialize");
    let parsed: WalletVerificationDoc = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.method, "memo_tx");
    assert_eq!(parsed.proof_sig, "5sigsig");
}

// -- mailbox_address matrix --------------------------------------------

#[test]
fn mailbox_address_base58_maps_to_mainnet_caip10() {
    let addr = mailbox_address(SOL_B58).expect("valid");
    assert_eq!(
        addr,
        format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2)
    );
}

/// Pins the key-only identity contract: ANY 32-byte ed25519 public key is
/// mailbox-addressable — no on-chain account, no funded wallet, no
/// existence check. This is what lets an agent with only a keypair (e.g.
/// generated for messaging alone) verify and use the inbox; the docs
/// promise it, this test keeps the promise.
/// The support responder echoes the thread id of the message it answers.
/// A pairwise dm id of two CAIP-10s is ~172 bytes — it must round-trip
/// through validation even though caller-minted ids cap at 128.
#[test]
fn thread_validation_roundtrips_the_systems_own_dm_ids() {
    let a = format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2);
    let b = format!(
        "{}:{}",
        chain_registry::SOLANA_MAINNET_CAIP2,
        SUPPORT_WALLET
    );
    let dm = pairwise_thread_id(&a, &b);
    assert!(
        dm.len() > limits::MAX_ID_BYTES,
        "the bug requires a long dm id"
    );
    assert!(
        validate_thread_id(&dm).is_ok(),
        "self-minted ids must validate"
    );
    // Caller-minted ids keep the generic rules, with teaching in the error.
    assert!(validate_thread_id("task:abc").is_ok());
    let err = validate_thread_id(&"x".repeat(200)).expect_err("long non-dm rejects");
    assert!(err.contains("task:{id}"), "error teaches the format: {err}");
    assert!(
        validate_thread_id("dm:a/b").is_err(),
        "no '/' even in dm ids"
    );
}

/// Only the org's support identities may address a session-scoped guest;
/// everyone else gets guidance instead of a dead end. This is the reply
/// path for auto-answers to unproven senders.
#[test]
fn session_guests_are_addressable_by_support_only() {
    let to = "session:2c0141e4-5ca0-4184-855b-a4e69e9f4535";
    assert_eq!(
        resolve_recipient(to, true).expect("support may reply to a guest"),
        to
    );
    let err = resolve_recipient(to, false).expect_err("others get guidance");
    assert!(
        err.contains("omit to_wallet"),
        "teaches the support path: {err}"
    );
    // Wallet recipients still normalize exactly as before.
    assert_eq!(
        resolve_recipient(SOL_B58, false).expect("wallet ok"),
        format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2)
    );
}

#[test]
fn mailbox_address_accepts_any_ed25519_key_without_onchain_presence() {
    // A synthetic key that certainly has no on-chain account.
    let key = bs58::encode([7u8; 32]).into_string();
    let addr = mailbox_address(&key).expect("key-only identity is addressable");
    assert_eq!(
        addr,
        format!("{}:{key}", chain_registry::SOLANA_MAINNET_CAIP2)
    );
}

#[test]
fn mailbox_address_solana_caip10_canonicalizes_chain_ref_to_mainnet() {
    // A devnet CAIP-10 of the same key must land in the SAME mailbox as
    // its bare-base58 form — the mailbox identity is the key.
    let devnet = format!("{}:{SOL_B58}", chain_registry::SOLANA_DEVNET_CAIP2);
    let mainnet = format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2);
    assert_eq!(mailbox_address(&devnet).expect("valid"), mainnet);
    assert_eq!(mailbox_address(&mainnet).expect("valid"), mainnet);
}

#[test]
fn mailbox_address_bare_evm_matches_register_wallet_binding() {
    // The bare-0x form must equal the CAIP-10 register_wallet binds
    // (lowercased), so `to_wallet: 0x…` reaches the bound agent's mailbox.
    let bound = crate::xchain::evm_account_id(EVM_ADDR).expect("valid");
    assert_eq!(mailbox_address(EVM_ADDR).expect("valid"), bound);
}

#[test]
fn mailbox_address_evm_mixed_case_collapses_to_one_mailbox() {
    let lower = mailbox_address(EVM_ADDR).expect("valid");
    let mixed = mailbox_address("0x996213ed4099707059B8B5D7489FFF23DAC9770D").expect("valid");
    assert_eq!(lower, mixed, "EIP-55 case must not mint a second mailbox");
    let full = mailbox_address(&format!(
        "eip155:84532:{}",
        "0x996213ED4099707059b8b5d7489fff23dac9770d"
    ))
    .expect("valid");
    assert_eq!(lower, full);
}

#[test]
fn mailbox_address_rejects_malformed() {
    assert!(mailbox_address("").is_err());
    assert!(mailbox_address("0x1234").is_err(), "short EVM");
    assert!(mailbox_address("0xZZ96213ed4099707059b8b5d7489fff23dac9770").is_err());
    assert!(mailbox_address("not-a-wallet!!").is_err(), "not base58");
    assert!(
        mailbox_address("3J98t1WpEZ73CNm").is_err(),
        "base58 but not 32 bytes"
    );
    assert!(
        mailbox_address("cosmos:cosmoshub-4:cosmos1abc").is_err(),
        "unsupported namespace"
    );
}

#[test]
fn caip10_address_strips_chain_prefix() {
    assert_eq!(
        caip10_address(&format!(
            "{}:{SOL_B58}",
            chain_registry::SOLANA_MAINNET_CAIP2
        )),
        SOL_B58
    );
    assert_eq!(
        caip10_address(&format!("eip155:84532:{EVM_ADDR}")),
        EVM_ADDR
    );
    assert_eq!(caip10_address("no-colons"), "no-colons");
}

// -- msg id / cursor ordering ------------------------------------------

#[test]
fn msg_ids_are_chronologically_string_ordered() {
    // Property over a spread of timestamps: later time ⇒ lexicographically
    // greater id, regardless of the random suffix.
    let base = chrono::DateTime::parse_from_rfc3339("2026-08-24T00:00:00Z")
        .expect("valid")
        .with_timezone(&chrono::Utc);
    let mut prev: Option<String> = None;
    for step in [1i64, 10, 1_000, 1_000_000, 86_400_000_000] {
        let t = base
            .checked_add_signed(chrono::Duration::microseconds(step))
            .expect("no overflow");
        let id = new_msg_id(t);
        assert_eq!(id.len(), 29, "020-micros + '_' + 8 hex: {id}");
        if let Some(p) = prev {
            assert!(id > p, "{id} must sort after {p}");
        }
        prev = Some(id);
    }
}

#[test]
fn msg_id_same_instant_ids_differ() {
    let now = chrono::Utc::now();
    let a = new_msg_id(now);
    let b = new_msg_id(now);
    assert_ne!(a, b, "random suffix must break same-microsecond ties");
    // Same timestamp prefix, whichever suffix order.
    assert_eq!(a[..21], b[..21]);
}

// -- fast-path truth table ---------------------------------------------

fn meta(unread: i64, cursor: &str, watermark: &str) -> MailboxMetaDoc {
    MailboxMetaDoc {
        wallet: "w".to_string(),
        unread_count: unread,
        latest_cursor: cursor.to_string(),
        read_watermark: watermark.to_string(),
        updated_at: ts("2026-08-24T00:00:00Z"),
    }
}

#[test]
fn fast_path_truth_table() {
    // (unread, latest_cursor, read_watermark) → empty?
    let cases = [
        // No mailbox doc at all: empty.
        (None, true, "no doc"),
        // Fresh mailbox: no sends, nothing to read.
        (Some(meta(0, "", "")), true, "fresh"),
        // All acked: cursor == watermark.
        (Some(meta(0, "05_a", "05_a")), true, "fully acked"),
        // Unread hint set: NOT empty.
        (Some(meta(2, "05_a", "03_a")), false, "unread > 0"),
        // THE ack/send race: ack reset unread to 0 but a concurrent send
        // advanced latest_cursor past the watermark — the cursor guard
        // must force the full read.
        (Some(meta(0, "09_b", "05_a")), false, "ack/send race"),
        // Watermark ahead of cursor (acked a cursor from a filtered raw
        // page): still empty.
        (Some(meta(0, "05_a", "09_b")), true, "watermark ahead"),
    ];
    for (m, want, name) in cases {
        assert_eq!(mailbox_is_empty(m.as_ref()), want, "case: {name}");
    }
}

// -- tier matrix --------------------------------------------------------

#[test]
fn tier_matrix_and_send_limits() {
    // (session, wallet_doc, reputation) → tier
    let cases = [
        (false, false, false, SenderTier::Unproven),
        // No session proof: nothing else matters.
        (false, true, true, SenderTier::Unproven),
        (true, false, false, SenderTier::SessionVerified),
        // Reputation WITHOUT an on-chain proof does not upgrade.
        (true, false, true, SenderTier::SessionVerified),
        (true, true, false, SenderTier::WalletVerified),
        (true, true, true, SenderTier::Reputable),
    ];
    for (s, w, r, want) in cases {
        assert_eq!(resolve_tier(s, w, r), want, "({s},{w},{r})");
    }
    assert_eq!(SenderTier::Unproven.send_limit(), 0);
    assert_eq!(SenderTier::SessionVerified.send_limit(), 5);
    assert_eq!(SenderTier::WalletVerified.send_limit(), 100);
    assert_eq!(SenderTier::Reputable.send_limit(), 500);
}

#[test]
fn tier_post_limits_ladder() {
    // The board ladder mirrors the send ladder: proof raises the cap,
    // unproven cannot post at all, and each rung strictly dominates.
    assert_eq!(SenderTier::Unproven.post_limit(), 0);
    assert_eq!(SenderTier::SessionVerified.post_limit(), 5);
    assert_eq!(SenderTier::WalletVerified.post_limit(), 50);
    assert_eq!(SenderTier::Reputable.post_limit(), 200);
    assert!(SenderTier::SessionVerified.post_limit() < SenderTier::WalletVerified.post_limit());
    assert!(SenderTier::WalletVerified.post_limit() < SenderTier::Reputable.post_limit());
}

// -- intent / thread validation / bounds --------------------------------

#[test]
fn intent_enum_validation() {
    assert_eq!(parse_intent(None).expect("ok"), None);
    assert_eq!(parse_intent(Some("")).expect("ok"), None);
    for v in VALID_INTENTS {
        assert_eq!(parse_intent(Some(v)).expect("ok").as_deref(), Some(v));
    }
    let err = parse_intent(Some("payment_request")).expect_err("rejects unknown");
    assert!(
        err.contains("payment_request"),
        "names the bad value: {err}"
    );
}

#[test]
fn id_token_bounds() {
    assert!(validate_id_token("task:abc", "thread_id").is_ok());
    assert!(validate_id_token("", "thread_id").is_err());
    assert!(validate_id_token(&"x".repeat(limits::MAX_ID_BYTES), "t").is_ok());
    assert!(validate_id_token(&"x".repeat(limits::MAX_ID_BYTES.saturating_add(1)), "t").is_err());
    assert!(
        validate_id_token("a/b", "thread_id").is_err(),
        "slash would break the doc path"
    );
}

#[test]
fn pairwise_thread_id_is_direction_independent() {
    let a = format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2);
    let b = format!("eip155:84532:{EVM_ADDR}");
    assert_eq!(pairwise_thread_id(&a, &b), pairwise_thread_id(&b, &a));
    assert!(pairwise_thread_id(&a, &b).starts_with("dm:"));
}

#[test]
fn limits_are_the_locked_values() {
    // The plan locks these numbers; a drive-by "tune" should fail a test.
    assert_eq!(limits::MAX_BODY_BYTES, 4096);
    assert_eq!(limits::READS_PER_DAY, 5000);
    assert_eq!(limits::PAGE_DEFAULT, 20);
    assert_eq!(limits::PAGE_MAX, 50);
    assert_eq!(limits::THREAD_MESSAGE_CAP, 500);
    assert_eq!(limits::QUOTA_TTL_DAYS, 3);
    // W3 board dials (tunable, but a change must be deliberate).
    assert_eq!(limits::POSTS_PER_DAY_UNPROVEN, 0);
    assert_eq!(limits::POSTS_PER_DAY_SESSION_VERIFIED, 5);
    assert_eq!(limits::POSTS_PER_DAY_WALLET_VERIFIED, 50);
    assert_eq!(limits::POSTS_PER_DAY_REPUTABLE, 200);
    // Reach-the-org openness dials (unproven → support / public board).
    assert_eq!(limits::SENDS_PER_DAY_UNPROVEN_SUPPORT, 10);
    assert_eq!(limits::POSTS_PER_DAY_UNPROVEN_PUBLIC, 10);
    assert_eq!(limits::POST_TTL_DAYS, 30);
    assert_eq!(limits::REPORT_AUTO_HIDE_DISTINCT_REPORTERS, 3);
    assert!(
        limits::REPORTERS_TRACK_CAP > limits::REPORT_AUTO_HIDE_DISTINCT_REPORTERS as usize,
        "the reporter-list cap must never gate auto-hide"
    );
    // W4 webhook dials.
    assert_eq!(limits::WEBHOOK_AUTO_DISABLE_FAILURES, 5);
    assert_eq!(limits::WEBHOOK_HANDSHAKE_TIMEOUT_SECS, 10);
    assert_eq!(limits::WEBHOOK_HANDSHAKE_MAX_RESPONSE_BYTES, 16 * 1024);
    assert_eq!(limits::MAX_WEBHOOK_URL_BYTES, 2048);
}

#[test]
fn page_clamp() {
    // Mirrors the clamp in get_messages (kept as a pure expression here).
    let clamp = |l: Option<u32>| l.unwrap_or(limits::PAGE_DEFAULT).clamp(1, limits::PAGE_MAX);
    assert_eq!(clamp(None), 20);
    assert_eq!(clamp(Some(0)), 1);
    assert_eq!(clamp(Some(50)), 50);
    assert_eq!(clamp(Some(51)), 50);
    assert_eq!(clamp(Some(u32::MAX)), 50);
}

#[test]
fn quota_day_is_utc_yyyymmdd() {
    let t = chrono::DateTime::parse_from_rfc3339("2026-08-24T23:59:59Z")
        .expect("valid")
        .with_timezone(&chrono::Utc);
    assert_eq!(quota_day(t), "20260824");
}

// -- shared wire shapes (one serialization, two transports) ------------

#[test]
fn send_receipt_json_carries_the_tool_response_shape() {
    let receipt = SendReceipt {
        msg_id: "00001756000000000000_0a1b2c3d".to_string(),
        to: format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2),
        thread_id: "task:abc".to_string(),
        intent: Some("task_offer".to_string()),
        bytes: 12,
        expires_at: None,
        sends_remaining_today: 4,
    };
    let v = send_receipt_json(&receipt);
    assert_eq!(v["sent"], true);
    assert!(v["expires_at"].is_null());
    assert_eq!(v["msg_id"], receipt.msg_id);
    assert_eq!(v["thread_id"], "task:abc");
    assert_eq!(v["sends_remaining_today"], 4);
    // Key set is the wire contract shared with the REST twin.
    let keys: Vec<&str> = v
        .as_object()
        .expect("object")
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "expires_at",
            "msg_id",
            "sends_remaining_today",
            "sent",
            "thread_id",
            "to_wallet"
        ]
    );
}

#[test]
fn read_page_json_carries_the_tool_response_shape() {
    let page = ReadPage {
        messages: vec![MessageOut {
            msg_id: "m1".to_string(),
            from_wallet: "w".to_string(),
            to_wallet: "v".to_string(),
            thread_id: "t".to_string(),
            intent: None,
            body: "hi".to_string(),
            sent_at: "2026-08-24T00:00:00+00:00".to_string(),
            seed: false,
            direction: DIRECTION_RECEIVED.to_string(),
        }],
        next_cursor: Some("m1".to_string()),
        fast_path: false,
        filtered_below_min_trust: 0,
        filtered_muted: 2,
    };
    let v = read_page_json(&page);
    assert_eq!(v["count"], 1);
    assert_eq!(v["next_cursor"], "m1");
    assert_eq!(v["filtered_muted"], 2);
    assert_eq!(v["messages"][0]["msg_id"], "m1");
    assert!(v["reminder"]
        .as_str()
        .expect("reminder")
        .contains("never instructions"));
}

#[test]
fn ack_json_carries_the_tool_response_shape() {
    let v = ack_json("00000000000000000009_ffffffff");
    assert_eq!(v["acked"], true);
    assert_eq!(v["read_watermark"], "00000000000000000009_ffffffff");
}

#[test]
fn rejection_reasons_are_stable_log_tokens() {
    // The `reason` strings are queried by log-based metrics — renaming
    // one silently breaks the funnel dashboards.
    assert_eq!(
        InboxRejection::SendQuotaExceeded { limit: 5 }.reason(),
        "send_quota_exceeded"
    );
    assert_eq!(InboxRejection::ThreadMuted.reason(), "thread_muted");
    assert_eq!(InboxRejection::UnprovenSender.reason(), "unproven_sender");
    assert_eq!(
        InboxRejection::BodyTooLarge { bytes: 5000 }.reason(),
        "body_too_large"
    );
    assert_eq!(
        InboxRejection::InvalidTopic(String::new()).reason(),
        "invalid_topic"
    );
    assert_eq!(
        InboxRejection::PostQuotaExceeded { limit: 5 }.reason(),
        "post_quota_exceeded"
    );
    assert_eq!(InboxRejection::PostNotFound.reason(), "post_not_found");
    assert_eq!(
        InboxRejection::WalletProofRequired.reason(),
        "wallet_proof_required"
    );
    assert_eq!(
        InboxRejection::InvalidWebhookUrl(String::new()).reason(),
        "invalid_webhook_url"
    );
    assert_eq!(
        InboxRejection::WebhookChallengeFailed(String::new()).reason(),
        "webhook_challenge_failed"
    );
    assert_eq!(
        InboxRejection::WebhookNotFound.reason(),
        "webhook_not_found"
    );
    assert_eq!(
        InboxRejection::DeliveryIdMismatch.reason(),
        "delivery_id_mismatch"
    );
}

fn prov_fixture() -> SenderProvenance {
    SenderProvenance {
        client_ip: "203.0.113.7".to_string(),
        user_agent: "swarm-agent/1.0".to_string(),
        session_id: "sess-abc".to_string(),
    }
}

#[test]
fn send_reject_log_carries_recipient_and_provenance() {
    // A bounced send must record WHICH address it tried to reach AND from
    // whom (IP + UA + session), all on the one `agent_message_rejected`
    // line — the whole point of Change A.
    let f = RejectionLogFields::build(
        "send_quota_exceeded",
        Some(
            "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp:CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY",
        ),
        None,
        &prov_fixture(),
    );
    assert_eq!(f.reason, "send_quota_exceeded");
    assert_eq!(
        f.to.as_deref(),
        Some(
            "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp:CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY"
        )
    );
    assert_eq!(f.topic_id, None);
    assert_eq!(f.client_ip, "203.0.113.7");
    assert_eq!(f.user_agent, "swarm-agent/1.0");
    assert_eq!(f.session_id, "sess-abc");
}

#[test]
fn post_reject_log_carries_topic_not_recipient() {
    let f = RejectionLogFields::build(
        "post_quota_exceeded",
        None,
        Some("open-challenge"),
        &prov_fixture(),
    );
    assert_eq!(f.to, None);
    assert_eq!(f.topic_id.as_deref(), Some("open-challenge"));
    assert_eq!(f.session_id, "sess-abc");
}

#[test]
fn read_reject_log_omits_recipient_but_keeps_provenance() {
    // Reads (get/ack/mute/report/webhook) have no recipient — provenance
    // only, and `to` must be omitted, never logged empty.
    let f = RejectionLogFields::build("unproven_sender", None, None, &prov_fixture());
    assert_eq!(f.to, None);
    assert_eq!(f.topic_id, None);
    assert_eq!(f.client_ip, "203.0.113.7");
    assert_eq!(f.user_agent, "swarm-agent/1.0");
    assert_eq!(f.session_id, "sess-abc");
}

// -- W2: merge-cursor property (no skip / no dup) -----------------------

fn mk_msg(id: u64, direction: &str) -> InboxMessageDoc {
    InboxMessageDoc {
        schema: MESSAGE_SCHEMA.to_string(),
        msg_id: format!("{id:020}_00000000"),
        from_wallet: "a".to_string(),
        to_wallet: "b".to_string(),
        thread_id: "t".to_string(),
        intent: None,
        body: "x".to_string(),
        sent_at: ts("2026-08-24T00:00:00Z"),
        seed: false,
        direction: direction.to_string(),
    }
}

/// Simulate one bounded DESC Firestore page over an in-memory source.
fn simulated_page(
    source: &[InboxMessageDoc],
    cursor: Option<&str>,
    page_size: usize,
) -> Vec<InboxMessageDoc> {
    let mut rows: Vec<InboxMessageDoc> = source
        .iter()
        .filter(|m| cursor.is_none_or(|c| m.msg_id.as_str() < c))
        .cloned()
        .collect();
    rows.sort_by(|x, y| y.msg_id.cmp(&x.msg_id));
    rows.truncate(page_size);
    rows
}

/// Property: walking pages via merge_pages_desc visits EVERY message in
/// the union exactly once, in strict DESC order, for adversarial
/// interleavings and every small page size — the two-source no-skip /
/// no-dup guarantee the plan demands.
#[test]
fn merge_pagination_walks_every_message_once() {
    let interleavings: [(&[u64], &[u64]); 5] = [
        // The case where "min of the two per-source truncation cursors"
        // WOULD skip id 83 at page_size 2 — the last-emitted-raw cursor
        // must not.
        (&[100, 80], &[95, 83]),
        // Dense sent side under a sparse inbound side.
        (&[100, 40, 30], &[95, 94, 93, 92, 91, 90, 50]),
        // One side empty (include_sent with an empty outbox).
        (&[10, 9, 8, 7], &[]),
        (&[], &[6, 5, 4]),
        // Fully interleaved.
        (&[100, 90, 80, 70, 60], &[99, 89, 79, 69, 59]),
    ];
    for (inbound_ids, sent_ids) in interleavings {
        let inbound: Vec<InboxMessageDoc> = inbound_ids
            .iter()
            .map(|&i| mk_msg(i, DIRECTION_RECEIVED))
            .collect();
        let sent: Vec<InboxMessageDoc> = sent_ids
            .iter()
            .map(|&i| mk_msg(i, DIRECTION_SENT))
            .collect();
        let mut expected: Vec<String> = inbound
            .iter()
            .chain(sent.iter())
            .map(|m| m.msg_id.clone())
            .collect();
        expected.sort_by(|a, b| b.cmp(a));

        for page_size in 1..=6usize {
            let mut seen: Vec<String> = Vec::new();
            let mut cursor: Option<String> = None;
            // Bounded walk (rule 2): can never need more pages than
            // messages.
            let max_pages = expected.len().saturating_add(2);
            for _ in 0..max_pages {
                let a = simulated_page(&inbound, cursor.as_deref(), page_size);
                let b = simulated_page(&sent, cursor.as_deref(), page_size);
                let (page, next) = merge_pages_desc(a, b, page_size);
                for m in &page {
                    seen.push(m.msg_id.clone());
                }
                match next {
                    Some(c) => cursor = Some(c),
                    None => break,
                }
            }
            assert_eq!(
                    seen, expected,
                    "exact once-each DESC walk (inbound {inbound_ids:?} sent {sent_ids:?} page {page_size})"
                );
        }
    }
}

#[test]
fn merge_dedupes_self_send_pairs_keeping_the_received_copy() {
    // Sending to yourself lands both the inbox copy and the mirror under
    // ONE parent with the SAME msg_id — the merge must emit it once, as
    // the received copy.
    let inbound = vec![
        mk_msg(10, DIRECTION_RECEIVED),
        mk_msg(5, DIRECTION_RECEIVED),
    ];
    let sent = vec![mk_msg(10, DIRECTION_SENT), mk_msg(7, DIRECTION_SENT)];
    let (page, next) = merge_pages_desc(inbound, sent, 10);
    let got: Vec<(&str, &str)> = page
        .iter()
        .map(|m| (m.msg_id.as_str(), m.direction.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("00000000000000000010_00000000", "received"),
            ("00000000000000000007_00000000", "sent"),
            ("00000000000000000005_00000000", "received"),
        ]
    );
    assert!(next.is_none(), "neither source full, nothing truncated");
}

#[test]
fn merge_single_source_matches_legacy_cursor_semantics() {
    // include_sent=false degenerates to the pre-W2 behavior: cursor set
    // iff the raw page filled.
    let inbound: Vec<InboxMessageDoc> = (1..=3)
        .rev()
        .map(|i| mk_msg(i, DIRECTION_RECEIVED))
        .collect();
    let (page, next) = merge_pages_desc(inbound.clone(), Vec::new(), 3);
    assert_eq!(page.len(), 3);
    assert_eq!(next.as_deref(), Some("00000000000000000001_00000000"));
    let (page, next) = merge_pages_desc(inbound, Vec::new(), 4);
    assert_eq!(page.len(), 3);
    assert!(next.is_none());
}

// -- W2: sent-side filter skip ------------------------------------------

#[test]
fn inbound_filters_skip_sent_mirrors() {
    let now = ts("2026-08-24T00:00:00Z").0;
    let mut muted = std::collections::HashSet::new();
    muted.insert("t".to_string());
    // No trust scores at all: every inbound sender scores 0.
    let trust: std::collections::HashMap<String, f64> = Default::default();

    let raw = vec![
        mk_msg(10, DIRECTION_SENT),    // muted thread + zero trust: KEPT
        mk_msg(9, DIRECTION_RECEIVED), // muted: dropped
        mk_msg(8, DIRECTION_RECEIVED), // muted: dropped (also zero trust)
    ];
    let (out, filtered_trust, filtered_muted) =
        build_read_page_messages(raw, &muted, &trust, Some(0.5), now);
    assert_eq!(out.len(), 1, "only the sent mirror survives");
    assert_eq!(out[0].direction, "sent");
    assert_eq!(filtered_muted, 2, "muted counts inbound only");
    assert_eq!(filtered_trust, 0, "muted filter ran first");

    // Same page, no mute: the two inbound drop on trust, sent kept.
    let (out, filtered_trust, filtered_muted) = build_read_page_messages(
        vec![
            mk_msg(10, DIRECTION_SENT),
            mk_msg(9, DIRECTION_RECEIVED),
            mk_msg(8, DIRECTION_RECEIVED),
        ],
        &Default::default(),
        &trust,
        Some(0.5),
        now,
    );
    assert_eq!(out.len(), 1);
    assert_eq!((filtered_trust, filtered_muted), (2, 0));
}

#[test]
fn legacy_expiry_does_not_hide_either_inbox_direction() {
    let now = ts("2026-08-24T00:00:00Z").0;
    let legacy = |message: InboxMessageDoc| {
        let mut value = serde_json::to_value(message).unwrap();
        value["expires_at"] = "2026-08-23T00:00:00Z".into();
        serde_json::from_value::<InboxMessageDoc>(value).unwrap()
    };
    let expired_sent = legacy(mk_msg(10, DIRECTION_SENT));
    let expired_recv = legacy(mk_msg(9, DIRECTION_RECEIVED));
    let (out, _, _) = build_read_page_messages(
        vec![expired_sent, expired_recv, mk_msg(8, DIRECTION_RECEIVED)],
        &Default::default(),
        &Default::default(),
        None,
        now,
    );
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].msg_id, "00000000000000000010_00000000");
}

// -- W3: topics, moderation, post filtering -----------------------------

#[test]
fn topic_gate_accepts_only_the_seeded_topics() {
    assert!(validate_topic("open-challenge").is_ok());
    assert!(validate_topic("subcontract").is_ok());
    assert!(validate_topic("town-square").is_ok());
    for bad in [
        "",
        "open-challenge/x",
        "general",
        "OPEN-CHALLENGE",
        "wallet:abc",
    ] {
        assert!(validate_topic(bad).is_err(), "{bad:?} must be rejected");
    }
}

#[test]
fn post_intent_enum_validation() {
    assert_eq!(parse_post_intent(None).expect("ok"), None);
    for v in VALID_POST_INTENTS {
        assert_eq!(parse_post_intent(Some(v)).expect("ok").as_deref(), Some(v));
    }
    // Board intents are NOT valid message intents — the two enums stay
    // separate surfaces.
    assert!(parse_intent(Some("open_challenge")).is_err());
    assert!(parse_post_intent(Some("payment_request")).is_err());
}

#[test]
fn apply_report_distinct_reporters_hit_the_auto_hide_threshold() {
    // First distinct reporter.
    let (r1, c1, h1) = apply_report(&[], 0, "w1").expect("counts");
    assert_eq!((c1, h1), (1, false));
    // Duplicate: idempotent no-op.
    assert!(apply_report(&r1, c1, "w1").is_none());
    // Second distinct.
    let (r2, c2, h2) = apply_report(&r1, c1, "w2").expect("counts");
    assert_eq!((c2, h2), (2, false));
    // Third distinct reporter crosses the threshold → auto-hide.
    let (r3, c3, h3) = apply_report(&r2, c2, "w3").expect("counts");
    assert_eq!((c3, h3), (3, true));
    assert_eq!(r3.len(), 3);
    // Beyond the threshold it stays hidden.
    let (_, c4, h4) = apply_report(&r3, c3, "w4").expect("counts");
    assert_eq!((c4, h4), (4, true));
}

#[test]
fn apply_report_reporter_list_cap_is_a_noop_guard() {
    let full: Vec<String> = (0..limits::REPORTERS_TRACK_CAP)
        .map(|i| format!("w{i}"))
        .collect();
    let count = u32::try_from(full.len()).expect("small");
    assert!(
        apply_report(&full, count, "fresh").is_none(),
        "at the cap, further reports no-op (post hid long ago)"
    );
}

#[test]
fn hidden_and_expired_posts_are_dropped_on_read() {
    let now = ts("2026-08-24T00:00:00Z").0;
    let post = |id: u64, hidden: bool, author: &str| TopicPostDoc {
        schema: MESSAGE_SCHEMA.to_string(),
        post_id: format!("{id:020}_00000000"),
        topic_id: "open-challenge".to_string(),
        author_wallet: format!("{}:{author}", chain_registry::SOLANA_MAINNET_CAIP2),
        body: "x".to_string(),
        reply_to: None,
        intent: None,
        ref_id: None,
        reported_count: if hidden { 3 } else { 0 },
        reporters: vec![],
        hidden,
        created_at: ts("2026-08-24T00:00:00Z"),
        expires_at: ts("2126-01-01T00:00:00Z"),
        seed: false,
    };
    let mut expired = post(5, false, "a");
    expired.expires_at = ts("2026-08-23T00:00:00Z");
    let mut trust = std::collections::HashMap::new();
    trust.insert("a".to_string(), 0.9);
    trust.insert("b".to_string(), 0.1);

    let raw = vec![
        post(10, true, "a"),
        post(9, false, "a"),
        post(8, false, "b"),
        expired,
    ];
    let (posts, filtered_hidden, filtered_trust) = build_post_page(raw, &trust, Some(0.5), now);
    assert_eq!(posts.len(), 1, "hidden + low-trust + expired all dropped");
    assert_eq!(posts[0].post_id, "00000000000000000009_00000000");
    assert_eq!(filtered_hidden, 1);
    assert_eq!(filtered_trust, 1);

    // Without a floor, only hidden/expired drop.
    let raw = vec![
        post(10, true, "a"),
        post(9, false, "a"),
        post(8, false, "b"),
    ];
    let (posts, filtered_hidden, filtered_trust) =
        build_post_page(raw, &Default::default(), None, now);
    assert_eq!(posts.len(), 2);
    assert_eq!((filtered_hidden, filtered_trust), (1, 0));
}

#[test]
fn topic_post_doc_roundtrip_and_moderation_defaults() {
    let doc = TopicPostDoc {
        schema: MESSAGE_SCHEMA.to_string(),
        post_id: "00001756000000000000_0a1b2c3d".to_string(),
        topic_id: "subcontract".to_string(),
        author_wallet: format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2),
        body: "handing off task 42".to_string(),
        reply_to: Some("00001755000000000000_00000001".to_string()),
        intent: Some("subcontract_offer".to_string()),
        ref_id: Some("task:42".to_string()),
        reported_count: 2,
        reporters: vec!["w1".to_string(), "w2".to_string()],
        hidden: false,
        created_at: ts("2026-08-24T00:00:00Z"),
        expires_at: ts("2026-09-23T00:00:00Z"),
        seed: true,
    };
    let json = serde_json::to_string(&doc).expect("serialize");
    let parsed: TopicPostDoc = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.intent.as_deref(), Some("subcontract_offer"));
    assert_eq!(parsed.ref_id.as_deref(), Some("task:42"));
    assert_eq!(parsed.reporters.len(), 2);

    // A doc written before any report: moderation fields default.
    let bare = r#"{
            "schema":"swarm/v1","post_id":"p1","topic_id":"open-challenge",
            "author_wallet":"w","body":"gm",
            "created_at":"2026-08-24T00:00:00Z","expires_at":"2026-09-23T00:00:00Z",
            "seed":false
        }"#;
    let parsed: TopicPostDoc = serde_json::from_str(bare).expect("bare deserializes");
    assert_eq!(parsed.reported_count, 0);
    assert!(parsed.reporters.is_empty() && !parsed.hidden);
    assert!(parsed.reply_to.is_none() && parsed.intent.is_none() && parsed.ref_id.is_none());
}

// -- W4: SSRF matrix, handshake, HMAC, delivery results -----------------

#[test]
fn webhook_url_ssrf_rejection_matrix() {
    let rejected = [
        "http://example.com/hook",                              // non-https
        "https://10.0.0.1/hook",                                // rfc1918
        "https://172.16.5.5/hook",                              // rfc1918
        "https://172.31.255.255/hook",                          // rfc1918 upper edge
        "https://192.168.1.1/hook",                             // rfc1918
        "https://127.0.0.1/hook",                               // loopback
        "https://169.254.169.254/latest",                       // metadata IP
        "https://0.0.0.0/hook",                                 // unspecified
        "https://0.1.2.3/hook",                                 // 0.0.0.0/8
        "https://100.64.0.1/hook",                              // CGNAT
        "https://100.127.255.255/hook",                         // CGNAT upper edge
        "https://192.0.0.9/hook",                               // 192.0.0.0/24
        "https://[::1]/hook",                                   // v6 loopback
        "https://[fc00::1]/hook",                               // v6 unique-local
        "https://[fdab::1]/hook",                               // v6 unique-local
        "https://[fe80::1]/hook",                               // v6 link-local
        "https://[::ffff:10.0.0.1]/hook",                       // v4-mapped private
        "https://localhost/hook",                               // loopback name
        "https://foo.localhost/hook",                           // *.localhost
        "https://my-svc.run.app/hook",                          // *.run.app
        "https://mcp-server-abc123.a.run.app/hook",             // *.run.app
        "https://db.internal/hook",                             // *.internal
        "https://metadata.google.internal/computeMetadata/v1/", // metadata
        "https://printer.local/hook",                           // *.local
        "https://user:pass@example.com/hook",                   // embedded creds
        "not a url",
        "",
    ];
    for url in rejected {
        assert!(
            validate_webhook_url(url).is_err(),
            "{url:?} must be rejected"
        );
    }
    let accepted = [
        "https://example.com/hook",
        "https://agent.example.com/inbox/webhook?token=abc",
        "https://8.8.8.8/hook",     // public IP literal is fine
        "https://172.32.0.1/hook",  // just past the 172.16/12 block
        "https://100.128.0.1/hook", // just past CGNAT
    ];
    for url in accepted {
        assert!(
            validate_webhook_url(url).is_ok(),
            "{url:?} must be accepted"
        );
    }
}

#[test]
fn ip_screen_covers_resolved_addresses() {
    use std::net::IpAddr;
    let forbidden: [IpAddr; 6] = [
        "10.1.2.3".parse().expect("ip"),
        "169.254.169.254".parse().expect("ip"),
        "127.0.0.1".parse().expect("ip"),
        "::1".parse().expect("ip"),
        "fe80::1".parse().expect("ip"),
        "::ffff:192.168.0.1".parse().expect("ip"),
    ];
    for ip in forbidden {
        assert!(ip_is_forbidden(ip), "{ip} must be forbidden");
    }
    let fine: [IpAddr; 3] = [
        "8.8.8.8".parse().expect("ip"),
        "104.16.0.1".parse().expect("ip"),
        "2606:4700::1111".parse().expect("ip"),
    ];
    for ip in fine {
        assert!(!ip_is_forbidden(ip), "{ip} must be allowed");
    }
}

#[tokio::test]
async fn handshake_accepts_a_2xx_body_echoing_the_token() {
    use wiremock::matchers::{body_partial_json, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    let token = "deadbeef".repeat(8);
    Mock::given(method("POST"))
        .and(body_partial_json(serde_json::json!({
            "type": "swarm_webhook_challenge",
            "token": token,
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "token": token, "ok": true })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = reqwest::Client::new();
    perform_challenge_handshake(&client, &server.uri(), &token)
        .await
        .expect("echo match verifies");
}

#[tokio::test]
async fn handshake_rejects_wrong_echo_and_non_2xx() {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let client = reqwest::Client::new();

    // Wrong token in the body: NOT verified.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("some-other-token"))
        .expect(1)
        .mount(&server)
        .await;
    let err = perform_challenge_handshake(&client, &server.uri(), "expected-token")
        .await
        .expect_err("mismatched echo");
    assert!(err.contains("did not echo"), "{err}");

    // 2xx is required — a 500 echoing the token is still a failure.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("tok-1"))
        .expect(1)
        .mount(&server)
        .await;
    let err = perform_challenge_handshake(&client, &server.uri(), "tok-1")
        .await
        .expect_err("500 fails");
    assert!(err.contains("500"), "{err}");
}

#[test]
fn webhook_signature_is_the_contracted_header_shape() {
    // RFC 4231-derivable check: deterministic, prefixed, hex, and
    // keyed — a different secret or payload changes the digest.
    let sig = webhook_signature("secret-a", r#"{"event":"inbox_message"}"#);
    let again = webhook_signature("secret-a", r#"{"event":"inbox_message"}"#);
    assert_eq!(sig, again, "deterministic over identical bytes");
    let (prefix, hexpart) = sig.split_at(7);
    assert_eq!(prefix, "sha256=");
    assert_eq!(hexpart.len(), 64, "hex-encoded 32-byte digest");
    assert!(hexpart.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(
        sig,
        webhook_signature("secret-b", r#"{"event":"inbox_message"}"#)
    );
    assert_ne!(
        sig,
        webhook_signature("secret-a", r#"{"event":"inbox_message"} "#)
    );
    // Known vector, verifiable with `echo -n <payload> | openssl dgst
    // -sha256 -hmac <key>`: pins the algorithm choice itself.
    assert_eq!(
        webhook_signature("key", "The quick brown fox jumps over the lazy dog"),
        "sha256=f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
}

// -- support-responder trigger -----------------------------------------

#[test]
fn support_sender_gets_tier_short_circuit_in_any_form() {
    // The resolve_sender_tier short-circuit's pure core: BOTH the dedicated
    // support wallet and the root/treasury wallet resolve as the support
    // sender in every accepted input form; other wallets and garbage never
    // do.
    assert!(is_support_sender(SUPPORT_WALLET));
    assert!(is_support_sender(&format!(
        "{}:{SUPPORT_WALLET}",
        chain_registry::SOLANA_MAINNET_CAIP2
    )));
    assert!(is_support_sender(ROOT_B58));
    assert!(is_support_sender(&format!(
        "{}:{ROOT_B58}",
        chain_registry::SOLANA_MAINNET_CAIP2
    )));
    assert!(!is_support_sender(SOL_B58));
    assert!(!is_support_sender("not-a-wallet"));
    assert!(!is_support_sender(""));
}

#[test]
fn support_wallet_matches_both_wallets_and_rejects_others() {
    // The recipient-matching predicate is a SET: the dedicated support
    // wallet AND the root/treasury wallet both resolve to support, in
    // base58, full CAIP-10, and whitespace-padded forms; unrelated wallets
    // do not.
    for wallet in [SUPPORT_WALLET, ROOT_B58] {
        let canonical = mailbox_address(wallet).expect("support wallet is valid base58");
        assert!(is_support_mailbox(&canonical), "{wallet} base58 → support");
        let full_caip10 = format!("{}:{wallet}", chain_registry::SOLANA_MAINNET_CAIP2);
        assert!(
            is_support_mailbox(&mailbox_address(&full_caip10).expect("caip10 form parses")),
            "{wallet} caip10 → support"
        );
        assert!(
            is_support_mailbox(
                &mailbox_address(&format!("  {wallet}  ")).expect("padded form parses")
            ),
            "{wallet} padded → support"
        );
    }
    // A different Solana wallet and an EVM wallet are NOT the support box.
    assert!(!is_support_mailbox(
        &mailbox_address(SOL_B58).expect("other solana wallet")
    ));
    assert!(!is_support_mailbox(
        &mailbox_address(EVM_ADDR).expect("evm wallet")
    ));
}

#[test]
fn support_self_reply_guard_recognizes_the_support_sender() {
    // notify_support_responder skips when the SENDER is the support wallet
    // (no self-reply loop). The guard is `is_support_mailbox(from)`, so a
    // message FROM the support wallet — in any input form — trips it.
    let from = mailbox_address(SUPPORT_WALLET).expect("support wallet");
    assert!(is_support_mailbox(&from), "from == support → guard fires");
    // The root/treasury wallet is also a support identity → guard fires
    // (from == either support wallet → skip the responder).
    let root = mailbox_address(ROOT_B58).expect("root wallet");
    assert!(is_support_mailbox(&root), "from == root → guard fires");
    // A normal sender does not trip the guard.
    let other = mailbox_address(SOL_B58).expect("other wallet");
    assert!(!is_support_mailbox(&other));
}

// -- reach-the-org openness (unproven → support / public board) --------

#[test]
fn default_recipient_is_the_dedicated_support_wallet() {
    // An omitted to_wallet routes to the dedicated support wallet (never
    // the root) — the mailbox the responder actually watches.
    let default = default_recipient_wallet();
    assert!(is_support_sender(&default), "default resolves to support");
    assert_eq!(
        mailbox_address(&default).expect("valid"),
        mailbox_address(SUPPORT_WALLET).expect("valid"),
        "default is the dedicated support wallet, not the root"
    );
}

#[test]
fn synthetic_session_sender_is_not_mailbox_addressable() {
    // The synthetic unproven-sender id must be a valid, stable, non-empty
    // author string that can NEVER be parsed as a real mailbox (so it can
    // never receive a reply) and is stable per session.
    let a = synthetic_session_sender("sess-abc");
    assert_eq!(a, "session:sess-abc");
    assert_eq!(
        a,
        synthetic_session_sender("sess-abc"),
        "stable per session"
    );
    assert_ne!(a, synthetic_session_sender("sess-xyz"), "distinct sessions");
    assert!(!a.is_empty());
    assert!(
        mailbox_address(&a).is_err(),
        "synthetic sender must not be mailbox-addressable"
    );
}

#[test]
fn public_topic_gate_is_town_square_only() {
    assert!(is_public_topic("town-square"));
    assert!(!is_public_topic("open-challenge"));
    assert!(!is_public_topic("subcontract"));
    assert!(!is_public_topic("nonsense"));
}

#[test]
fn effective_send_limit_opens_support_only_for_unproven() {
    // Unproven → support: the small trickle; unproven → non-support: still
    // 0 (agent-to-agent hard gate). Verified tiers are unchanged either way.
    assert_eq!(
        effective_send_limit(SenderTier::Unproven, true),
        limits::SENDS_PER_DAY_UNPROVEN_SUPPORT
    );
    assert_eq!(effective_send_limit(SenderTier::Unproven, false), 0);
    assert_eq!(
        effective_send_limit(SenderTier::SessionVerified, true),
        limits::SENDS_PER_DAY_SESSION_VERIFIED
    );
    assert_eq!(
        effective_send_limit(SenderTier::WalletVerified, false),
        limits::SENDS_PER_DAY_WALLET_VERIFIED
    );
    assert_eq!(
        effective_send_limit(SenderTier::Reputable, true),
        limits::SENDS_PER_DAY_REPUTABLE
    );
}

#[test]
fn unproven_support_send_is_allowed_up_to_cap_then_rejected() {
    // Mirrors the send_message quota guard (`sends_used >= limit` rejects)
    // for the unproven→support path: sends 0..9 (already-used counts) pass,
    // the 10th already-used send is rejected (SendQuotaExceeded).
    let limit = effective_send_limit(SenderTier::Unproven, true);
    assert_eq!(limit, 10);
    // Pure predicate identical to the one in send_message.
    let rejects = |sends_used: i64| sends_used >= i64::from(limit);
    for sends_used in 0..i64::from(limit) {
        assert!(
            !rejects(sends_used),
            "send #{sends_used} within cap accepted"
        );
    }
    assert!(rejects(i64::from(limit)), "the (cap+1)th send is rejected");
    // An unproven sender to a NON-support wallet never even reaches the
    // quota check — the effective limit is 0 → UnprovenSender.
    assert_eq!(effective_send_limit(SenderTier::Unproven, false), 0);
}

#[test]
fn effective_post_limit_opens_public_topic_only_for_unproven() {
    assert_eq!(
        effective_post_limit(SenderTier::Unproven, true),
        limits::POSTS_PER_DAY_UNPROVEN_PUBLIC
    );
    assert_eq!(effective_post_limit(SenderTier::Unproven, false), 0);
    assert_eq!(
        effective_post_limit(SenderTier::SessionVerified, true),
        limits::POSTS_PER_DAY_SESSION_VERIFIED
    );
    assert_eq!(
        effective_post_limit(SenderTier::Reputable, false),
        limits::POSTS_PER_DAY_REPUTABLE
    );
}

#[test]
fn unproven_public_post_allowed_up_to_cap_non_public_zero() {
    let public = effective_post_limit(SenderTier::Unproven, true);
    assert_eq!(public, 10, "town-square: 10/day for unproven");
    // Non-public topic stays hard-gated at 0 for unproven.
    assert_eq!(effective_post_limit(SenderTier::Unproven, false), 0);
}

#[test]
fn responder_payload_and_signature_pin_to_known_bytes() {
    // The bridge verifies X-Swarm-Responder-Signature over the EXACT body
    // bytes; pin both the serialized shape and the HMAC so a change to
    // either breaks here, not in production.
    let payload = responder_payload_json("wallet-from", "task:42", "m1", "need help");
    assert_eq!(
        payload,
        r#"{"body":"need help","from_wallet":"wallet-from","msg_id":"m1","thread_id":"task:42"}"#
    );
    // The 4 fields the bridge needs, decodable back from the raw bytes.
    let decoded: serde_json::Value = serde_json::from_str(&payload).expect("valid json");
    assert_eq!(decoded["from_wallet"], "wallet-from");
    assert_eq!(decoded["thread_id"], "task:42");
    assert_eq!(decoded["msg_id"], "m1");
    assert_eq!(decoded["body"], "need help");
    // Known-answer HMAC (openssl dgst -sha256 -hmac <secret> over payload).
    assert_eq!(
        webhook_signature("shared-responder-secret", &payload),
        "sha256=5bb2438d6985c8b0be589f6704d2c8d6724cbd4e5e33c1cdf89e55ee34fb083f"
    );
}

#[test]
fn plan_support_responder_covers_every_branch() {
    let support = mailbox_address(SUPPORT_WALLET).expect("support wallet");
    let other = mailbox_address(SOL_B58).expect("other wallet");
    let url = "https://bridge.example.run.app/webhook/inbox";
    let secret = "shared-responder-secret";

    // Happy path: to == support, from != support, url + secret present.
    let post = plan_support_responder(
        &support,
        &other,
        "task:42",
        "m1",
        "need help",
        url,
        Some(secret),
    )
    .expect("configured support message triggers");
    assert_eq!(post.url, url);
    assert_eq!(
        post.body,
        responder_payload_json(&other, "task:42", "m1", "need help"),
        "signed body matches the wire body"
    );
    assert_eq!(post.signature, webhook_signature(secret, &post.body));

    // Not the support mailbox → no trigger.
    assert!(
        plan_support_responder(&other, &other, "t", "m", "hi", url, Some(secret)).is_none(),
        "non-support recipient never triggers"
    );

    // Self-reply loop guard: FROM the support wallet → no trigger.
    assert!(
        plan_support_responder(&support, &support, "t", "m", "hi", url, Some(secret)).is_none(),
        "support→support never triggers"
    );

    // Not configured: empty url → no trigger, no error.
    assert!(
        plan_support_responder(&support, &other, "t", "m", "hi", "", Some(secret)).is_none(),
        "empty url disables the trigger"
    );

    // Not configured: missing secret → no trigger (never POST unsigned).
    assert!(
        plan_support_responder(&support, &other, "t", "m", "hi", url, None).is_none(),
        "missing secret disables the trigger"
    );
}

#[test]
fn delivery_result_counter_and_auto_disable() {
    // Failures accumulate to the threshold, then disable.
    let mut failures = 0i64;
    for i in 1..limits::WEBHOOK_AUTO_DISABLE_FAILURES {
        let (f, disable) = apply_delivery_result(failures, false);
        assert_eq!((f, disable), (i, false), "below threshold");
        failures = f;
    }
    let (f, disable) = apply_delivery_result(failures, false);
    assert_eq!(
        (f, disable),
        (limits::WEBHOOK_AUTO_DISABLE_FAILURES, true),
        "threshold disables"
    );
    // A delivered outcome resets the counter from any depth.
    assert_eq!(apply_delivery_result(4, true), (0, false));
    assert_eq!(apply_delivery_result(0, true), (0, false));
}

#[test]
fn webhook_doc_roundtrip_and_defaults() {
    let doc = WebhookDoc {
        wallet: format!("{}:{SOL_B58}", chain_registry::SOLANA_MAINNET_CAIP2),
        url: "https://agent.example.com/hook".to_string(),
        hmac_secret: "ab".repeat(32),
        challenge_token: "cd".repeat(32),
        verified: true,
        consecutive_failures: 2,
        disabled_at: None,
        last_delivery_at: Some(ts("2026-08-24T00:00:00Z")),
        pending_delivery_id: "d1".to_string(),
        created_at: ts("2026-08-24T00:00:00Z"),
    };
    let json = serde_json::to_string(&doc).expect("serialize");
    let parsed: WebhookDoc = serde_json::from_str(&json).expect("deserialize");
    assert!(parsed.verified);
    assert_eq!(parsed.consecutive_failures, 2);
    assert_eq!(parsed.pending_delivery_id, "d1");

    // Registration-time doc: outcome-owned fields default.
    let bare = r#"{"wallet":"w","url":"https://x.example/h","hmac_secret":"s",
                 "challenge_token":"t","verified":true,
                 "created_at":"2026-08-24T00:00:00Z"}"#;
    let parsed: WebhookDoc = serde_json::from_str(bare).expect("bare deserializes");
    assert_eq!(parsed.consecutive_failures, 0);
    assert!(parsed.disabled_at.is_none() && parsed.last_delivery_at.is_none());
    assert_eq!(parsed.pending_delivery_id, "");
}

// -- new wire shapes ----------------------------------------------------

#[test]
fn post_receipt_and_page_json_shapes() {
    let receipt = PostReceipt {
        post_id: "p1".to_string(),
        topic_id: "open-challenge".to_string(),
        reply_to: Some("p0".to_string()),
        intent: Some("open_challenge".to_string()),
        bytes: 2,
        expires_at: ts("2026-09-23T00:00:00Z").0,
        posts_remaining_today: 4,
    };
    let v = post_receipt_json(&receipt);
    assert_eq!(v["published"], true);
    assert_eq!(v["post_id"], "p1");
    assert_eq!(v["reply_to"], "p0");
    assert_eq!(v["posts_remaining_today"], 4);

    let page = PostPage {
        topic_id: "open-challenge".to_string(),
        posts: vec![],
        next_cursor: None,
        filtered_hidden: 1,
        filtered_below_min_trust: 0,
    };
    let v = post_page_json(&page);
    assert_eq!(v["count"], 0);
    assert_eq!(v["filtered_hidden"], 1);
    assert!(v["reminder"]
        .as_str()
        .expect("reminder")
        .contains("never instructions"));
}

#[test]
fn report_and_webhook_json_shapes() {
    let outcome = ReportOutcome {
        topic_id: "subcontract".to_string(),
        post_id: "p1".to_string(),
        reported_count: 3,
        hidden: true,
        already_reported: false,
    };
    let v = report_outcome_json(&outcome);
    assert_eq!(v["hidden"], true);
    assert_eq!(v["reported_count"], 3);

    let doc = WebhookDoc {
        wallet: "w".to_string(),
        url: "https://agent.example.com/hook".to_string(),
        hmac_secret: "sek".to_string(),
        challenge_token: "tok".to_string(),
        verified: true,
        consecutive_failures: 0,
        disabled_at: None,
        last_delivery_at: None,
        pending_delivery_id: String::new(),
        created_at: ts("2026-08-24T00:00:00Z"),
    };
    let v = webhook_json(&doc);
    assert_eq!(v["verified"], true);
    assert_eq!(v["hmac_secret"], "sek");
    assert!(v["signature_scheme"]
        .as_str()
        .expect("scheme")
        .contains("X-Swarm-Signature"));
    assert!(
        v.get("challenge_token").is_none(),
        "the challenge token is not re-exposed"
    );
}
