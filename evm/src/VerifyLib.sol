// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title VerifyLib — canonical verification-descriptor + attestation encoding
/// @notice Solidity mirror of `crates/chain-core/src/verify_schema.rs` — the
///         cross-VM contract for the deterministic-verification engine.
///         Product-neutral: scoping to a deployment comes from `chainTag` +
///         `contractId` + `subjectId` inside the signed payload. Every field
///         is one 32-byte big-endian word; `keccak256(abi.encode(...))` here
///         is byte-identical to the Rust word-concat encoding, held equal by
///         the golden vectors in `tests/fixtures/verify-vectors.json`.
///         Field order is the wire format — never reorder; `verificationKind`
///         values are append-only.
library VerifyLib {
    /// keccak256("SWARM_VERIFY_V1")
    bytes32 internal constant VERIFY_MAGIC = 0x7eff0baa43b6e1ec3b5aeab422b0e67a20040c062b6c03b9f87b17209ba25b7c;
    /// keccak256("SWARM_ATTEST_V1")
    bytes32 internal constant ATTEST_MAGIC = 0xbf58eddef16c0f8d19b1098873eb58cabb7aa98ca40b6e0aa03d9cd1d99634ec;

    uint16 internal constant VERIFY_SCHEMA_VERSION = 1;

    /// Verification kinds — numeric values are the cross-chain wire format
    /// AND the on-chain `Task.verification_kind` byte (append-only).
    uint8 internal constant KIND_ORACLE_METRICS = 0;
    uint8 internal constant KIND_DETERMINISTIC_ATTESTED = 1;

    /// Score scale shared with the Solana program (`shared::MAX_SCORE`).
    /// `KIND_DETERMINISTIC_ATTESTED` admits only {0, MAX_SCORE};
    /// `KIND_ORACLE_METRICS` admits the continuum.
    uint64 internal constant MAX_SCORE = 1_000_000;

    /// keccak256 of the exact bytes of `policies/lean-attester-policy-v1.json`
    /// (pinned toolchain, axiom allow-list, resource caps). A policy change is
    /// a NEW policy version with a new id, never a mutation of v1.
    bytes32 internal constant LEAN_POLICY_V1_ID = 0xd52a2aa68bdbc5f34d3acb0bc4dcdfd4936ea8ab930e6d0cd37174df19db1eab;

    /// What was verified, under which rules: the statement, the policy that
    /// constrains acceptable proofs of it, and the artifact that discharged it.
    struct Descriptor {
        uint8 verificationKind;
        bytes32 statementCommitment; // sha256 of the canonical statement source
        bytes32 policyId; // keccak256 of the exact policy-manifest bytes
        bytes32 artifactHash; // sha256 of the submitted artifact (0 until submit)
    }

    /// @notice 6 words, 192 bytes — identical to verify_schema.rs encode().
    function encodeDescriptor(Descriptor memory d) internal pure returns (bytes memory) {
        return abi.encode(
            VERIFY_MAGIC,
            uint256(VERIFY_SCHEMA_VERSION),
            uint256(d.verificationKind),
            d.statementCommitment,
            d.policyId,
            d.artifactHash
        );
    }

    function descriptorDigest(Descriptor memory d) internal pure returns (bytes32) {
        return keccak256(encodeDescriptor(d));
    }

    /// @notice 10 words, 320 bytes. The descriptor is embedded flattened (its
    ///         magic is not repeated — ATTEST_MAGIC domain-separates the whole
    ///         payload). This digest is what the allow-listed attester signs
    ///         (v = 27/28) to release payment for one subject on one
    ///         deployment of one chain.
    function encodeAttestation(
        bytes32 chainTag,
        bytes32 contractId,
        uint64 subjectId,
        Descriptor memory d,
        uint64 score
    ) internal pure returns (bytes memory) {
        return abi.encode(
            ATTEST_MAGIC,
            uint256(VERIFY_SCHEMA_VERSION),
            chainTag,
            contractId,
            uint256(subjectId),
            uint256(d.verificationKind),
            d.statementCommitment,
            d.policyId,
            d.artifactHash,
            uint256(score)
        );
    }

    function attestationDigest(
        bytes32 chainTag,
        bytes32 contractId,
        uint64 subjectId,
        Descriptor memory d,
        uint64 score
    ) internal pure returns (bytes32) {
        return keccak256(encodeAttestation(chainTag, contractId, subjectId, d, score));
    }
}
