//! Canonical verification-descriptor and attestation byte layout — the
//! cross-VM contract for the deterministic-verification engine.
//!
//! Product-neutral by design: the engine is the substrate and Shillbot is
//! vertical #1, so nothing here names a product. Scoping to a deployment
//! comes from `chain_tag` + `contract` + `subject_id` inside the signed
//! payload. Future verification categories (code-via-tests, NP-witness)
//! append new `VerificationKind` values — never reorder.
//!
//! Same rules as `cert_schema`: every field is one 32-byte big-endian
//! word, byte-identical to Solidity `abi.encode` for value types; the
//! Solidity mirror (`evm/src/VerifyLib.sol`) is held equal by the golden
//! vectors in `tests/fixtures/verify-vectors.json`.
//!
//! Signature conventions:
//! - EVM: the attester signs `AttestationCert::digest()` with
//!   `cosign::sign_digest_eth` (v = 27/28); the contract recovers via
//!   `ECDSA.recover` and compares against its allow-listed signer.
//! - Solana: the attester authorizes by signing the transaction as the
//!   on-chain `oracle_authority` `Signer`; the digest is stored in the
//!   task's `verification_hash` as the audit binding.

use crate::words::{push_u16, push_u64, push_u8, push_word};

/// keccak256("SWARM_VERIFY_V1") — asserted in tests.
pub const VERIFY_MAGIC: [u8; 32] = [
    0x7e, 0xff, 0x0b, 0xaa, 0x43, 0xb6, 0xe1, 0xec, 0x3b, 0x5a, 0xea, 0xb4, 0x22, 0xb0, 0xe6, 0x7a,
    0x20, 0x04, 0x0c, 0x06, 0x2b, 0x6c, 0x03, 0xb9, 0xf8, 0x7b, 0x17, 0x20, 0x9b, 0xa2, 0x5b, 0x7c,
];

/// keccak256("SWARM_ATTEST_V1") — asserted in tests.
pub const ATTEST_MAGIC: [u8; 32] = [
    0xbf, 0x58, 0xed, 0xde, 0xf1, 0x6c, 0x0f, 0x8d, 0x19, 0xb1, 0x09, 0x88, 0x73, 0xeb, 0x58, 0xca,
    0xbb, 0x7a, 0xa9, 0x8c, 0xa4, 0x0b, 0x6e, 0x0a, 0xa0, 0x3d, 0x9c, 0xd1, 0xd9, 0x96, 0x34, 0xec,
];

pub const VERIFY_SCHEMA_VERSION: u16 = 1;

/// Encoded sizes: every field is one 32-byte word.
pub const DESCRIPTOR_WORDS: usize = 6;
pub const ATTESTATION_WORDS: usize = 10;

/// keccak256 of the exact bytes of `policies/lean-attester-policy-v1.json`
/// — the v1 deterministic-Lean verification policy (pinned toolchain,
/// axiom allow-list, resource caps). Asserted against the file in tests;
/// changing the policy file means a NEW policy version + new id, never a
/// mutation of this one.
pub const LEAN_POLICY_V1_ID: [u8; 32] = [
    0xd5, 0x2a, 0x2a, 0xa6, 0x8b, 0xdb, 0xc5, 0xf3, 0x4d, 0x3a, 0xcb, 0x0b, 0xc4, 0xdc, 0xdf, 0xd4,
    0x93, 0x6e, 0xa8, 0xab, 0x93, 0x0e, 0x6d, 0x0c, 0xd3, 0x71, 0x74, 0xdf, 0x19, 0xdb, 0x1e, 0xab,
];

/// How a task's verification is adjudicated. The numeric values are part
/// of the cross-chain wire format AND the on-chain `Task.verification_kind`
/// byte — append-only, never reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VerificationKind {
    /// Legacy path: an oracle feed (Switchboard on Solana) or an
    /// attester-signed continuum score (EVM) reports a composite metric.
    OracleMetrics = 0,
    /// A deterministic checker (e.g. pinned-Lean proof check) re-runnable
    /// by anyone; the attester's signature releases a binary score.
    DeterministicAttested = 1,
}

impl VerificationKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::OracleMetrics),
            1 => Some(Self::DeterministicAttested),
            _ => None,
        }
    }
}

/// What was verified, under which rules: the statement, the policy that
/// constrains acceptable proofs of it, and the artifact that discharged it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyDescriptor {
    /// `VerificationKind` discriminant.
    pub verification_kind: u8,
    /// SHA-256 of the canonical statement source (e.g. `Statement.lean`).
    pub statement_commitment: [u8; 32],
    /// keccak256 of the exact policy-manifest bytes (toolchain pin, axiom
    /// allow-list, resource caps) — e.g. `LEAN_POLICY_V1_ID`.
    pub policy_id: [u8; 32],
    /// SHA-256 of the submitted artifact bytes (zero until submission).
    pub artifact_hash: [u8; 32],
}

impl VerifyDescriptor {
    /// Canonical payload: 6 words, 192 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(DESCRIPTOR_WORDS.saturating_mul(crate::words::WORD));
        push_word(&mut out, &VERIFY_MAGIC);
        push_u16(&mut out, VERIFY_SCHEMA_VERSION);
        push_u8(&mut out, self.verification_kind);
        push_word(&mut out, &self.statement_commitment);
        push_word(&mut out, &self.policy_id);
        push_word(&mut out, &self.artifact_hash);
        debug_assert_eq!(
            out.len(),
            DESCRIPTOR_WORDS.saturating_mul(crate::words::WORD)
        );
        out
    }
}

/// The attestation an allow-listed attester signs to release payment for
/// one subject (task) on one deployment of one chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestationCert {
    /// keccak256 of the CAIP-2 chain string (chain-registry convention).
    pub chain_tag: [u8; 32],
    /// Program ID (Solana, 32 bytes) or contract address (EVM, 20 bytes
    /// left-padded to 32) — same convention as `CertLeg.contract`.
    pub contract: [u8; 32],
    /// The task/subject id within that deployment.
    pub subject_id: u64,
    pub descriptor: VerifyDescriptor,
    /// Verification score. `DeterministicAttested` admits only 0 or
    /// MAX_SCORE (1_000_000); `OracleMetrics` admits the continuum.
    pub score: u64,
}

impl AttestationCert {
    /// Canonical payload: 10 words, 320 bytes. The descriptor is embedded
    /// flattened (its own magic is not repeated — `ATTEST_MAGIC` domain-
    /// separates the whole payload).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(ATTESTATION_WORDS.saturating_mul(crate::words::WORD));
        push_word(&mut out, &ATTEST_MAGIC);
        push_u16(&mut out, VERIFY_SCHEMA_VERSION);
        push_word(&mut out, &self.chain_tag);
        push_word(&mut out, &self.contract);
        push_u64(&mut out, self.subject_id);
        push_u8(&mut out, self.descriptor.verification_kind);
        push_word(&mut out, &self.descriptor.statement_commitment);
        push_word(&mut out, &self.descriptor.policy_id);
        push_word(&mut out, &self.descriptor.artifact_hash);
        push_u64(&mut out, self.score);
        debug_assert_eq!(
            out.len(),
            ATTESTATION_WORDS.saturating_mul(crate::words::WORD)
        );
        out
    }

    /// The signing digest (EVM: `sign_digest_eth`; Solana: stored as
    /// `verification_hash`).
    #[cfg(feature = "keccak")]
    pub fn digest(&self) -> [u8; 32] {
        crate::cert_schema::keccak256(&self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_descriptor() -> VerifyDescriptor {
        VerifyDescriptor {
            verification_kind: VerificationKind::DeterministicAttested as u8,
            statement_commitment: [0x51; 32],
            policy_id: LEAN_POLICY_V1_ID,
            artifact_hash: [0x53; 32],
        }
    }

    fn sample_attestation() -> AttestationCert {
        AttestationCert {
            chain_tag: [0x61; 32],
            contract: [0x62; 32],
            subject_id: 42,
            descriptor: sample_descriptor(),
            score: 1_000_000,
        }
    }

    #[test]
    fn encoded_sizes_are_fixed() {
        assert_eq!(sample_descriptor().encode().len(), 192);
        assert_eq!(sample_attestation().encode().len(), 320);
    }

    #[test]
    fn encoding_is_field_order_sensitive() {
        let base = sample_descriptor();
        let mut swapped = base.clone();
        core::mem::swap(&mut swapped.statement_commitment, &mut swapped.policy_id);
        assert_ne!(base.encode(), swapped.encode());

        let base = sample_attestation();
        let mut swapped = base.clone();
        core::mem::swap(&mut swapped.chain_tag, &mut swapped.contract);
        assert_ne!(base.encode(), swapped.encode());
    }

    #[test]
    fn small_ints_are_left_padded_be_words() {
        let bytes = sample_attestation().encode();
        // Word 1 is the schema version: 30 zero bytes then 0x00 0x01.
        let version_word = &bytes[32..64];
        assert!(version_word[..30].iter().all(|b| *b == 0));
        assert_eq!(&version_word[30..], &[0x00, 0x01]);
        // Last word is score = 1_000_000 = 0x0F4240.
        let last = &bytes[bytes.len() - 32..];
        assert!(last[..29].iter().all(|b| *b == 0));
        assert_eq!(&last[29..], &[0x0F, 0x42, 0x40]);
    }

    #[test]
    fn verification_kind_round_trips_and_rejects_unknown() {
        for value in 0..=1u8 {
            let kind = VerificationKind::from_u8(value).expect("known kind");
            assert_eq!(kind as u8, value);
        }
        assert_eq!(VerificationKind::from_u8(2), None);
        assert_eq!(VerificationKind::from_u8(255), None);
    }

    #[cfg(feature = "keccak")]
    #[test]
    fn magic_constants_match_their_preimages() {
        use crate::cert_schema::keccak256;
        assert_eq!(keccak256(b"SWARM_VERIFY_V1"), VERIFY_MAGIC);
        assert_eq!(keccak256(b"SWARM_ATTEST_V1"), ATTEST_MAGIC);
    }

    #[cfg(feature = "keccak")]
    #[test]
    fn lean_policy_id_pins_the_manifest_bytes() {
        use crate::cert_schema::keccak256;
        let manifest = include_bytes!("../../../policies/lean-attester-policy-v1.json");
        assert_eq!(
            keccak256(manifest),
            LEAN_POLICY_V1_ID,
            "policy manifest bytes changed — a policy change is a NEW policy \
             version with a new id, never a mutation of v1"
        );
    }

    #[cfg(feature = "keccak")]
    #[test]
    fn digest_binds_every_field() {
        let base = sample_attestation();
        let base_digest = base.digest();

        let mut other = base.clone();
        other.subject_id = 43;
        assert_ne!(other.digest(), base_digest);

        let mut other = base.clone();
        other.score = 0;
        assert_ne!(other.digest(), base_digest);

        let mut other = base.clone();
        other.descriptor.artifact_hash = [0x54; 32];
        assert_ne!(other.digest(), base_digest);

        let mut other = base.clone();
        other.descriptor.verification_kind = VerificationKind::OracleMetrics as u8;
        assert_ne!(other.digest(), base_digest);
    }
}
