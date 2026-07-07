// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {VerifyLib} from "../src/VerifyLib.sol";

/// @notice Proves the Solidity verification-schema encoder is byte-for-byte
///         identical to the Rust `verify_schema` encoder, and that the
///         attester signatures authored by `cosign::sign_digest_eth`
///         recover to the expected signer via `ecrecover`. Vectors are
///         authored by
///         `cargo test -p chain-core --features cosign --test verify_vectors`.
contract VerifyLibTest is Test {
    string internal constant FIXTURE = "../tests/fixtures/verify-vectors.json";

    function _fill(uint8 b) internal pure returns (bytes32 out) {
        for (uint256 i = 0; i < 32; i++) {
            out |= bytes32(uint256(b)) << (8 * i);
        }
    }

    /// Mirrors chain-core verify_vectors.rs `deterministic_descriptor()`.
    function _deterministicDescriptor() internal pure returns (VerifyLib.Descriptor memory) {
        return VerifyLib.Descriptor({
            verificationKind: VerifyLib.KIND_DETERMINISTIC_ATTESTED,
            statementCommitment: _fill(0x51),
            policyId: VerifyLib.LEAN_POLICY_V1_ID,
            artifactHash: _fill(0x53)
        });
    }

    /// Mirrors chain-core verify_vectors.rs `oracle_descriptor()`.
    function _oracleDescriptor() internal pure returns (VerifyLib.Descriptor memory) {
        return VerifyLib.Descriptor({
            verificationKind: VerifyLib.KIND_ORACLE_METRICS,
            statementCommitment: _fill(0xA1),
            policyId: _fill(0xA2),
            artifactHash: _fill(0xA3)
        });
    }

    struct DescriptorVector {
        bytes payload;
        bytes32 digest;
    }

    struct AttestationVector {
        bytes payload;
        bytes32 digest;
        bytes signature;
        address signer;
    }

    function _readDescriptor(string memory key) internal view returns (DescriptorVector memory v) {
        string memory json = vm.readFile(FIXTURE);
        v.payload = vm.parseJsonBytes(json, string.concat(".descriptors.", key, ".payload"));
        v.digest = vm.parseJsonBytes32(json, string.concat(".descriptors.", key, ".digest"));
    }

    function _readAttestation(string memory key) internal view returns (AttestationVector memory v) {
        string memory json = vm.readFile(FIXTURE);
        v.payload = vm.parseJsonBytes(json, string.concat(".attestations.", key, ".payload"));
        v.digest = vm.parseJsonBytes32(json, string.concat(".attestations.", key, ".digest"));
        v.signature = vm.parseJsonBytes(json, string.concat(".attestations.", key, ".signature"));
        v.signer = vm.parseJsonAddress(json, string.concat(".attestations.", key, ".signer"));
    }

    function _assertAttestation(string memory key, VerifyLib.Descriptor memory d, uint64 score) internal view {
        // chain_tag 0x61-fill, contract 0x62-fill, subject_id 42 — mirrors
        // verify_vectors.rs `attestation()`.
        bytes memory payload = VerifyLib.encodeAttestation(_fill(0x61), _fill(0x62), 42, d, score);
        AttestationVector memory v = _readAttestation(key);
        assertEq(payload, v.payload, string.concat(key, ": attestation payload drift vs Rust encoder"));
        bytes32 digest = VerifyLib.attestationDigest(_fill(0x61), _fill(0x62), 42, d, score);
        assertEq(digest, v.digest, string.concat(key, ": attestation digest drift"));

        // The sign_digest_eth signature (v = 27|28) recovers to the signer.
        (bytes32 r, bytes32 s, uint8 vByte) = _splitSig(v.signature);
        assertTrue(vByte == 27 || vByte == 28, "v must be 27|28 for ecrecover");
        assertEq(ecrecover(digest, vByte, r, s), v.signer, string.concat(key, ": ecrecover signer mismatch"));
    }

    function _splitSig(bytes memory sig) internal pure returns (bytes32 r, bytes32 s, uint8 v) {
        assertEq(sig.length, 65, "signature must be [r||s||v]");
        assembly {
            r := mload(add(sig, 0x20))
            s := mload(add(sig, 0x40))
            v := byte(0, mload(add(sig, 0x60)))
        }
    }

    function test_deterministicDescriptor_matches_golden_vector() public view {
        bytes memory payload = VerifyLib.encodeDescriptor(_deterministicDescriptor());
        DescriptorVector memory v = _readDescriptor("deterministic");
        assertEq(payload, v.payload, "deterministic descriptor payload drift vs Rust encoder");
        assertEq(VerifyLib.descriptorDigest(_deterministicDescriptor()), v.digest, "descriptor digest drift");
        assertEq(payload.length, 192, "descriptor must be 6 words");
    }

    function test_oracleDescriptor_matches_golden_vector() public view {
        bytes memory payload = VerifyLib.encodeDescriptor(_oracleDescriptor());
        DescriptorVector memory v = _readDescriptor("oracle");
        assertEq(payload, v.payload, "oracle descriptor payload drift vs Rust encoder");
        assertEq(VerifyLib.descriptorDigest(_oracleDescriptor()), v.digest, "descriptor digest drift");
    }

    function test_attestation_pass_matches_golden_vector_and_recovers() public view {
        _assertAttestation("pass", _deterministicDescriptor(), VerifyLib.MAX_SCORE);
    }

    function test_attestation_fail_matches_golden_vector_and_recovers() public view {
        _assertAttestation("fail", _deterministicDescriptor(), 0);
    }

    function test_attestation_oracle_matches_golden_vector_and_recovers() public view {
        _assertAttestation("oracle", _oracleDescriptor(), 734_210);
    }

    function test_attestation_payload_is_10_words() public pure {
        bytes memory payload = VerifyLib.encodeAttestation(_fill(0x61), _fill(0x62), 42, _deterministicDescriptor(), 1);
        assertEq(payload.length, 320, "attestation must be 10 words");
    }

    function test_magic_constants_match_preimages() public pure {
        assertEq(VerifyLib.VERIFY_MAGIC, keccak256("SWARM_VERIFY_V1"));
        assertEq(VerifyLib.ATTEST_MAGIC, keccak256("SWARM_ATTEST_V1"));
    }

    function test_digest_binds_score_and_subject() public pure {
        VerifyLib.Descriptor memory d = _deterministicDescriptor();
        bytes32 base = VerifyLib.attestationDigest(_fill(0x61), _fill(0x62), 42, d, VerifyLib.MAX_SCORE);
        assertTrue(base != VerifyLib.attestationDigest(_fill(0x61), _fill(0x62), 42, d, 0), "score not bound");
        assertTrue(
            base != VerifyLib.attestationDigest(_fill(0x61), _fill(0x62), 43, d, VerifyLib.MAX_SCORE),
            "subject not bound"
        );
        assertTrue(
            base != VerifyLib.attestationDigest(_fill(0x61), _fill(0x63), 42, d, VerifyLib.MAX_SCORE),
            "contract not bound"
        );
    }
}
