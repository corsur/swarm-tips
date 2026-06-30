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
        // Distinct multipliers + offsets so no stake/tranche aliases another
        // leg's: a 2x tranche made leg_a.tranche (0x10*2M) collide with
        // leg_b.stake (0x20*1M), letting an encode() field-swap slip the golden
        // vector. 3x + distinct offsets makes every numeric word unique.
        stake: u128::from(seed).wrapping_mul(1_000_000).wrapping_add(11),
        tranche: u128::from(seed).wrapping_mul(3_000_000).wrapping_add(22),
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
        // a_is_p1 = 0 (exercises the non-default seat) — also distinct from every
        // other scalar so a field swap into this slot changes the digest.
        a_is_p1: 0,
    }
}

fn sample_checkpoint(match_live_digest: [u8; 32]) -> Checkpoint {
    // Every small-int field a DISTINCT value so reordering any two in encode()
    // changes the bytes (M10). 255 = UNREVEALED, exercising that sentinel too.
    Checkpoint {
        match_live_digest,
        step_count: 3,
        p1_commit: [0xC1; 32],
        p2_commit: [0xC2; 32],
        p1_guess: 1,
        p2_guess: 255,
        first_committer: 2,
        matchup_type: 0,
        transcript_hash: [0xD0; 32],
        r_matchup: [0xE1; 32],
    }
}

fn sample_outcome(match_live_digest: [u8; 32]) -> OutcomeCert {
    OutcomeCert {
        match_id: [0xAA; 32],
        match_live_digest,
        outcome_kind: OutcomeKind::HeteroP2Wins,
        step_count: 4,
        p1_guess: 1,
        p2_guess: 255,
        first_committer: 2,
        matchup_type: 0,
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

// ---------------------------------------------------------------------------
// Outcome-derivation truth table (M11). The serialization vectors above pin the
// byte LAYOUT; this pins the DERIVATION — given a checkpoint's
// (step_count, guesses, first_committer, matchup_type), which OutcomeKind it
// resolves to. chain-core is the reference: it computes `expected_kind` here, and
// `CertLib.deriveClaimOutcome` (Solidity) reads the same file and must agree, so
// the Rust↔Solidity derivations can never silently diverge. The BPF copy
// (`programs/.../cert.rs::derive_claim_outcome`) and chain-core itself are pinned
// to the SAME inputs by exhaustive hand-authored unit tests in their own crates.
// ---------------------------------------------------------------------------

/// Exhaustive (step_count, p1_guess, p2_guess, first_committer, matchup_type)
/// rows covering every branch of `derive_outcome_kind` + `derive_terminal_outcome`.
/// 255 = UNREVEALED. Hand-authored inputs; chain-core computes the expected kind.
const DERIVATION_ROWS: &[(u8, u8, u8, u8, u8)] = &[
    // Terminal (step 4), homogeneous (matchup 0): correct = guess == 0.
    (4, 0, 0, 1, 0), // both correct -> HomogBothCorrect(0)
    (4, 0, 0, 2, 0), // first_committer irrelevant when homogeneous -> 0
    (4, 0, 1, 1, 0), // p1 correct only -> HomogP1Correct(1)
    (4, 1, 0, 1, 0), // p2 correct only -> HomogP2Correct(2)
    (4, 1, 1, 1, 0), // both wrong -> BothWrong(3)
    // Terminal (step 4), heterogeneous (matchup 1): correct = guess == 1.
    (4, 1, 0, 1, 1), // p1 correct only -> HeteroP1Wins(4)
    (4, 0, 1, 1, 1), // p2 correct only -> HeteroP2Wins(5)
    (4, 1, 1, 1, 1), // both correct, tie -> first_committer 1 -> HeteroP1Wins(4)
    (4, 1, 1, 2, 1), // both correct, tie -> first_committer 2 -> HeteroP2Wins(5)
    (4, 0, 0, 1, 1), // both wrong -> BothWrong(3)
    (4, 0, 0, 2, 1), // both wrong, first_committer irrelevant -> 3
    // Timeout step 0 (nobody committed) -> always TimeoutBothForfeit(8).
    (0, 255, 255, 255, 1),
    (0, 255, 255, 0, 0),
    // Timeout step 1 (one commit): committer wins; bad committer -> forfeit.
    (1, 255, 255, 1, 1), // TimeoutP1Wins(6)
    (1, 255, 255, 2, 1), // TimeoutP2Wins(7)
    (1, 255, 255, 0, 1), // inconsistent -> TimeoutBothForfeit(8)
    (1, 255, 255, 3, 1), // inconsistent -> 8
    // Timeout step 2 (both committed, none revealed) -> forfeit.
    (2, 255, 255, 1, 1),
    (2, 255, 255, 2, 0),
    // Timeout step 3 (both committed, one reveals): sole revealer wins.
    (3, 1, 255, 1, 1),   // p1 revealed only -> TimeoutP1Wins(6)
    (3, 255, 1, 1, 1),   // p2 revealed only -> TimeoutP2Wins(7)
    (3, 255, 255, 1, 1), // neither revealed -> 8
    (3, 1, 0, 1, 1),     // both revealed (guard) -> 8
    (3, 0, 1, 2, 0),     // both revealed (guard) -> 8
];

fn derivation_cp(step: u8, p1: u8, p2: u8, first: u8, matchup: u8) -> Checkpoint {
    Checkpoint {
        match_live_digest: [0; 32],
        step_count: step,
        p1_commit: [0; 32],
        p2_commit: [0; 32],
        p1_guess: p1,
        p2_guess: p2,
        first_committer: first,
        matchup_type: matchup,
        transcript_hash: [0; 32],
        r_matchup: [0; 32],
    }
}

fn u8_json_array(vals: impl Iterator<Item = u8>) -> String {
    let parts: Vec<String> = vals.map(|v| v.to_string()).collect();
    format!("[{}]", parts.join(", "))
}

fn build_derivation_json() -> String {
    let col = |sel: fn(&(u8, u8, u8, u8, u8)) -> u8| u8_json_array(DERIVATION_ROWS.iter().map(sel));
    let kinds = u8_json_array(
        DERIVATION_ROWS
            .iter()
            .map(|r| derivation_cp(r.0, r.1, r.2, r.3, r.4).derive_outcome_kind() as u8),
    );
    format!(
        "{{\n  \"comment\": \"OutcomeKind derivation truth table. chain-core computes \
         expected_kind; CertLib.sol reads + asserts. Parallel arrays, equal length. \
         255 = UNREVEALED.\",\n  \"step_count\": {},\n  \"p1_guess\": {},\n  \"p2_guess\": {},\n  \
         \"first_committer\": {},\n  \"matchup_type\": {},\n  \"expected_kind\": {}\n}}\n",
        col(|r| r.0),
        col(|r| r.1),
        col(|r| r.2),
        col(|r| r.3),
        col(|r| r.4),
        kinds,
    )
}

fn derivation_fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/outcome-derivation.json")
}

#[test]
#[ignore = "regenerates the committed fixture; run intentionally"]
fn write_derivation_vectors() {
    let path = derivation_fixture_path();
    std::fs::create_dir_all(path.parent().expect("fixture dir")).expect("create fixtures dir");
    std::fs::write(&path, build_derivation_json()).expect("write outcome-derivation.json");
}

#[test]
fn verify_derivation_vectors() {
    let path = derivation_fixture_path();
    let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing {}: run the ignored write_derivation_vectors test to generate it ({e})",
            path.display()
        )
    });
    assert_eq!(
        on_disk,
        build_derivation_json(),
        "outcome-derivation.json is stale — chain-core's derivation changed; regenerate with the \
         ignored write_derivation_vectors test and confirm the Solidity CertLib mirror still passes"
    );
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
