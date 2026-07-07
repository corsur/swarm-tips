//! Golden verification-schema vectors — the cross-language contract for
//! `verify_schema` (descriptors + attestations + attester signatures).
//!
//! Runs with `--features cosign` (signatures need k256). The Solidity
//! `VerifyLib` forge tests read the SAME file and assert their
//! `keccak256(abi.encode(...))` payloads, digests, and `ecrecover`
//! results match byte-for-byte.
//!
//! Regenerate intentionally with:
//!   `cargo test -p chain-core --features cosign --test verify_vectors -- --ignored write_verify_vectors`

#![cfg(feature = "cosign")]

mod common;

use chain_core::cert_schema::keccak256;
use chain_core::cosign::{eth_address, sign_digest_eth};
use chain_core::verify_schema::{
    AttestationCert, VerificationKind, VerifyDescriptor, LEAN_POLICY_V1_ID,
};
use common::{assert_fixture_current, hex, write_fixture};

const FIXTURE: &str = "verify-vectors.json";

/// Fixed, publicly-known test key (31 zero bytes + 0x42) — same convention
/// as the cosign unit tests. NEVER a real key.
fn test_secret() -> [u8; 32] {
    let mut s = [0u8; 32];
    s[31] = 0x42;
    s
}

fn deterministic_descriptor() -> VerifyDescriptor {
    VerifyDescriptor {
        verification_kind: VerificationKind::DeterministicAttested as u8,
        statement_commitment: [0x51; 32],
        policy_id: LEAN_POLICY_V1_ID,
        artifact_hash: [0x53; 32],
    }
}

fn oracle_descriptor() -> VerifyDescriptor {
    VerifyDescriptor {
        verification_kind: VerificationKind::OracleMetrics as u8,
        statement_commitment: [0xA1; 32],
        policy_id: [0xA2; 32],
        artifact_hash: [0xA3; 32],
    }
}

fn attestation(descriptor: VerifyDescriptor, score: u64) -> AttestationCert {
    AttestationCert {
        // Distinct fill bytes so any field swap in encode() changes the digest.
        chain_tag: [0x61; 32],
        contract: [0x62; 32],
        subject_id: 42,
        descriptor,
        score,
    }
}

fn descriptor_json(d: &VerifyDescriptor) -> String {
    let payload = d.encode();
    format!(
        "{{\n      \"payload\": \"{}\",\n      \"digest\": \"{}\"\n    }}",
        hex(&payload),
        hex(&keccak256(&payload)),
    )
}

fn attestation_json(cert: &AttestationCert, secret: &[u8; 32]) -> String {
    let payload = cert.encode();
    let digest = cert.digest();
    let sig = sign_digest_eth(secret, &digest).expect("test key signs");
    format!(
        "{{\n      \"payload\": \"{}\",\n      \"digest\": \"{}\",\n      \"signature\": \"{}\",\n      \"signer\": \"{}\"\n    }}",
        hex(&payload),
        hex(&digest),
        hex(&sig),
        hex(&eth_address(secret).expect("test key address")),
    )
}

fn build_json() -> String {
    let sk = test_secret();
    format!(
        "{{\n  \"descriptors\": {{\n    \"deterministic\": {},\n    \"oracle\": {}\n  }},\n  \"attestations\": {{\n    \"pass\": {},\n    \"fail\": {},\n    \"oracle\": {}\n  }}\n}}\n",
        descriptor_json(&deterministic_descriptor()),
        descriptor_json(&oracle_descriptor()),
        attestation_json(&attestation(deterministic_descriptor(), 1_000_000), &sk),
        attestation_json(&attestation(deterministic_descriptor(), 0), &sk),
        attestation_json(&attestation(oracle_descriptor(), 734_210), &sk),
    )
}

#[test]
#[ignore = "regenerates the committed fixture; run intentionally"]
fn write_verify_vectors() {
    write_fixture(FIXTURE, &build_json());
}

#[test]
fn verify_verify_vectors() {
    assert_fixture_current(FIXTURE, &build_json(), "write_verify_vectors");
}
