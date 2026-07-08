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

mod common;

use chain_core::cert_schema::{
    keccak256, CertLeg, Checkpoint, MatchLiveCert, OutcomeCert, OutcomeKind,
};
use common::{assert_fixture_current, hex, write_fixture};

const CERT_FIXTURE: &str = "cert-vectors.json";
const DERIVATION_FIXTURE: &str = "outcome-derivation.json";

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

// ---------------------------------------------------------------------------
// Outcome-derivation truth table (M11). The serialization vectors above pin the
// byte LAYOUT; this pins the DERIVATION — given a checkpoint's
// (step_count, guesses, first_committer, matchup_type), which OutcomeKind it
// resolves to. chain-core is the reference: it computes `expected_kind` from the
// CANONICAL `cert_schema::DERIVATION_TRUTH_TABLE` here, `CertLib.deriveClaimOutcome`
// (Solidity) reads the resulting file, and the Solana BPF test
// (`programs/.../cert.rs::derive_claim_outcome`) iterates the SAME const — so all
// three VMs are pinned to ONE source with no hand-copied duplicate that can drift.
// ---------------------------------------------------------------------------

/// Exhaustive (step_count, p1_guess, p2_guess, first_committer, matchup_type)
/// covering every branch of `derive_outcome_kind` + `derive_terminal_outcome`.
/// 255 = UNREVEALED. The rows come from the CANONICAL
/// `chain_core::cert_schema::DERIVATION_TRUTH_TABLE` (single source pinned across
/// all VMs) — this generator emits its inputs, chain-core computes the expected
/// kind, and `CertLib.t.sol` reads the result.
use chain_core::cert_schema::DERIVATION_TRUTH_TABLE;

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
    type Row = (u8, u8, u8, u8, u8, u8);
    let col = |sel: fn(&Row) -> u8| u8_json_array(DERIVATION_TRUTH_TABLE.iter().map(sel));
    // chain-core recomputes the expected kind from the canonical inputs (the
    // const's own r.5 column is the independent hand-authored audit, checked in
    // cert_schema's unit test).
    let kinds = u8_json_array(
        DERIVATION_TRUTH_TABLE
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

#[test]
#[ignore = "regenerates the committed fixture; run intentionally"]
fn write_derivation_vectors() {
    write_fixture(DERIVATION_FIXTURE, &build_derivation_json());
}

#[test]
fn verify_derivation_vectors() {
    assert_fixture_current(
        DERIVATION_FIXTURE,
        &build_derivation_json(),
        "write_derivation_vectors",
    );
}

#[test]
#[ignore = "regenerates the committed fixture; run intentionally"]
fn write_vectors() {
    write_fixture(CERT_FIXTURE, &build_json());
}

#[test]
fn verify_vectors() {
    assert_fixture_current(CERT_FIXTURE, &build_json(), "write_vectors");
}
