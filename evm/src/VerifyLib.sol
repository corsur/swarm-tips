// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

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

    /// Platform identifier for deterministic RPC-quorum `OnChainAction` VOW tasks.
    uint8 internal constant PLATFORM_ONCHAIN_ACTION = 11;

    /// Score scale shared with the Solana program (`shared::MAX_SCORE`).
    /// `KIND_DETERMINISTIC_ATTESTED` admits only {0, MAX_SCORE};
    /// `KIND_ORACLE_METRICS` admits the continuum.
    uint64 internal constant MAX_SCORE = 1_000_000;

    /// secp256k1n / 2 — upper bound on canonical `s` (EIP-2 non-malleability).
    uint256 internal constant SECP256K1_HALF_ORDER = 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0;

    /// keccak256 of the exact bytes of `policies/lean-attester-policy-v1.json`
    /// (pinned toolchain, axiom allow-list, resource caps). A policy change is
    /// a NEW policy version with a new id, never a mutation of v1.
    bytes32 internal constant LEAN_POLICY_V1_ID = 0xd52a2aa68bdbc5f34d3acb0bc4dcdfd4936ea8ab930e6d0cd37174df19db1eab;

    /// keccak256 of the exact bytes of `policies/lean-attester-policy-v2.json`.
    bytes32 internal constant LEAN_POLICY_V2_ID = 0x3fd32d5ba49af64e26ecefc7262234e14732feedbebcceaaac3c998c1a4d4ea0;

    /// keccak256 of the exact bytes of `policies/lean-attester-policy-v3.json`.
    bytes32 internal constant LEAN_POLICY_V3_ID = 0x984853e2ec3a1c9dcc9866121120400f09dd9352863dcd0472807259872809a3;

    /// keccak256 of the exact bytes of `policies/lean-attester-policy-v4.json`.
    bytes32 internal constant LEAN_POLICY_V4_ID = 0xd6a00cfb261c7136c74c6f29d53a2cfcdbbca12d520c63c25b09b90569f20533;

    /// keccak256 of the exact bytes of `policies/onchain-action-policy-v1.json` —
    /// the v1 deterministic RPC-quorum `PlatformType::OnChainAction (11)` VOW policy.
    bytes32 internal constant ONCHAIN_ACTION_POLICY_V1_ID =
        0x68ebd4b18b7400c67bca5d46db6d16847070c4d883517a1fdeb5c399ccef3d9f;

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

    /// @notice Canonical strict secp256k1 recovery over a 65-byte `[r || s || v]`
    ///         signature (`cosign::sign_digest_eth`). Explicitly enforces:
    ///         - `sig.length == 65`
    ///         - `v == 27 || v == 28`
    ///         - `uint256(s) <= SECP256K1_HALF_ORDER` (EIP-2 non-malleability)
    ///         - `recovered != address(0)`
    function recoverSigner(bytes32 digest, bytes memory sig) internal pure returns (address) {
        if (sig.length != 65) revert ECDSA.ECDSAInvalidSignatureLength(sig.length);
        bytes32 r;
        bytes32 s;
        uint8 v;
        assembly ("memory-safe") {
            r := mload(add(sig, 0x20))
            s := mload(add(sig, 0x40))
            v := byte(0, mload(add(sig, 0x60)))
        }
        if (v != 27 && v != 28) revert ECDSA.ECDSAInvalidSignature();
        if (uint256(s) > SECP256K1_HALF_ORDER) revert ECDSA.ECDSAInvalidSignatureS(s);
        address recovered = ecrecover(digest, v, r, s);
        if (recovered == address(0)) revert ECDSA.ECDSAInvalidSignature();
        return recovered;
    }
}
