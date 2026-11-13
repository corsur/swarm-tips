// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
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

    function recoverExternal(bytes32 digest, bytes memory sig) external pure returns (address) {
        return VerifyLib.recoverSigner(digest, sig);
    }

    function test_policyAndPlatformConstants_matchVerifySchema() public pure {
        assertEq(
            VerifyLib.ONCHAIN_ACTION_POLICY_V1_ID,
            0x68ebd4b18b7400c67bca5d46db6d16847070c4d883517a1fdeb5c399ccef3d9f,
            "onchain-action-policy-v1 id"
        );
        assertEq(
            VerifyLib.LEAN_POLICY_V2_ID,
            0x3fd32d5ba49af64e26ecefc7262234e14732feedbebcceaaac3c998c1a4d4ea0,
            "lean-policy-v2 id"
        );
        assertEq(
            VerifyLib.LEAN_POLICY_V3_ID,
            0x984853e2ec3a1c9dcc9866121120400f09dd9352863dcd0472807259872809a3,
            "lean-policy-v3 id"
        );
        assertEq(
            VerifyLib.LEAN_POLICY_V4_ID,
            0xd6a00cfb261c7136c74c6f29d53a2cfcdbbca12d520c63c25b09b90569f20533,
            "lean-policy-v4 id"
        );
        assertEq(VerifyLib.PLATFORM_ONCHAIN_ACTION, 11, "PlatformType::OnChainAction == 11");
    }

    function test_recoverSigner_enforcesValidV_lowS_andNonZeroAddress() public {
        uint256 pk = 0xBEEF1234;
        address expected = vm.addr(pk);
        bytes32 digest = keccak256("phase-4-1-strict-ecdsa");
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, digest);
        bytes memory validSig = abi.encodePacked(r, s, v);
        assertEq(this.recoverExternal(digest, validSig), expected, "canonical signature recovers");

        // Reject non-65-byte signatures.
        vm.expectRevert(abi.encodeWithSelector(ECDSA.ECDSAInvalidSignatureLength.selector, 64));
        this.recoverExternal(digest, new bytes(64));

        // Reject v != 27 && v != 28 (e.g. raw parity 0/1 or EIP-155 v = 29).
        uint8[4] memory badVs = [uint8(0), uint8(1), uint8(26), uint8(29)];
        for (uint256 i = 0; i < badVs.length; i++) {
            bytes memory badVSig = abi.encodePacked(r, s, badVs[i]);
            vm.expectRevert(ECDSA.ECDSAInvalidSignature.selector);
            this.recoverExternal(digest, badVSig);
        }

        // Reject malleable upper-half s > secp256k1n / 2.
        uint256 curveOrder = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141;
        bytes32 highS = bytes32(curveOrder - uint256(s));
        uint8 flippedV = v == 27 ? 28 : 27;
        bytes memory malleableSig = abi.encodePacked(r, highS, flippedV);
        vm.expectRevert(abi.encodeWithSelector(ECDSA.ECDSAInvalidSignatureS.selector, highS));
        this.recoverExternal(digest, malleableSig);

        // Reject invalid r = 0 (ecrecover returns address(0)).
        bytes memory zeroRSig = abi.encodePacked(bytes32(0), s, v);
        vm.expectRevert(ECDSA.ECDSAInvalidSignature.selector);
        this.recoverExternal(digest, zeroRSig);
    }
}
