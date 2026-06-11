//! Golden certificate vectors — the cross-language contract.
//!
//! This test (run with `--features keccak`) writes deterministic
//! certificate payloads + keccak digests to
//! `tests/fixtures/cert-vectors.json`. The Solidity `CertLib` forge tests
//! read the SAME file and assert their `keccak256(abi.encode(...))`
//! matches byte-for-byte. If either encoder drifts, one side's test
//! fails — the two implementations can never silently diverge.
//!
//! Regenerate intentionally with:
//!   `cargo test -p chain-core --features keccak --test golden_vectors -- --ignored write_vectors`
//! The default `verify_vectors` test fails if the committed file is stale.

#![cfg(feature = "keccak")]

use chain_core::cert_schema::{
    keccak256, CertLeg, Checkpoint, MatchLiveCert, OutcomeCert, OutcomeKind,
};

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().saturating_mul(2).saturating_add(2));
    s.push_str("0x");
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn leg(seed: u8) -> CertLeg {
    CertLeg {
        chain_tag: [seed; 32],
        contract: [seed.wrapping_add(1); 32],
        player: [seed.wrapping_add(2); 32],
        session_key: [seed.wrapping_add(3); 20],
        stake: u128::from(seed).wrapping_mul(1_000_000),
        tranche: u128::from(seed).wrapping_mul(2_000_000),
    }
}

fn sample_match_live() -> MatchLiveCert {
    MatchLiveCert {
        match_id: [0xAA; 32],
        tournament_id: 7,
        matchup_commitment: [0xBB; 32],
        leg_a: leg(0x10),
        leg_b: leg(0x20),
        quote_timestamp: 1_765_000_000,
        quote_max_age_secs: 300,
        match_deadline: 1_765_000_900,
        claim_window_secs: 3600,
        a_is_p1: 1,
    }
}

fn sample_checkpoint(match_live_digest: [u8; 32]) -> Checkpoint {
    Checkpoint {
        match_live_digest,
        step_count: 4,
        p1_commit: [0xC1; 32],
        p2_commit: [0xC2; 32],
        p1_guess: 1,
        p2_guess: 0,
        first_committer: 1,
        matchup_type: 1,
        transcript_hash: [0xD0; 32],
    }
}

fn sample_outcome(match_live_digest: [u8; 32]) -> OutcomeCert {
    OutcomeCert {
        match_id: [0xAA; 32],
        match_live_digest,
        outcome_kind: OutcomeKind::HeteroP1Wins,
        step_count: 4,
        p1_guess: 1,
        p2_guess: 0,
        first_committer: 1,
        matchup_type: 1,
        transcript_hash: [0xD0; 32],
    }
}

/// Build the canonical JSON document from the sample certs.
fn build_json() -> String {
    let ml = sample_match_live();
    let ml_payload = ml.encode();
    let ml_digest = keccak256(&ml_payload);

    let cp = sample_checkpoint(ml_digest);
    let cp_payload = cp.encode();
    let cp_digest = keccak256(&cp_payload);

    let oc = sample_outcome(ml_digest);
    let oc_payload = oc.encode();
    let oc_digest = keccak256(&oc_payload);

    // Hand-rolled JSON (no serde dep needed for this test). Stable key
    // order so the file is diff-friendly.
    format!(
        "{{\n  \"match_live\": {{\n    \"payload\": \"{}\",\n    \"digest\": \"{}\"\n  }},\n  \"checkpoint\": {{\n    \"payload\": \"{}\",\n    \"digest\": \"{}\"\n  }},\n  \"outcome\": {{\n    \"payload\": \"{}\",\n    \"digest\": \"{}\"\n  }}\n}}\n",
        hex(&ml_payload),
        hex(&ml_digest),
        hex(&cp_payload),
        hex(&cp_digest),
        hex(&oc_payload),
        hex(&oc_digest),
    )
}

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cert-vectors.json")
}

#[test]
#[ignore = "regenerates the committed fixture; run intentionally"]
fn write_vectors() {
    let path = fixture_path();
    std::fs::create_dir_all(path.parent().expect("fixture dir")).expect("create fixtures dir");
    std::fs::write(&path, build_json()).expect("write cert-vectors.json");
}

#[test]
fn verify_vectors() {
    let path = fixture_path();
    let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing {}: run the ignored write_vectors test to generate it ({e})",
            path.display()
        )
    });
    assert_eq!(
        on_disk,
        build_json(),
        "cert-vectors.json is stale — the Rust encoder changed; regenerate with the \
         ignored write_vectors test and update the Solidity mirror if needed"
    );
}
