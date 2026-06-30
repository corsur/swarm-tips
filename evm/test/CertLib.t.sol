// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CertLib} from "../src/CertLib.sol";

/// @notice Proves the Solidity certificate encoder is byte-for-byte
///         identical to the Rust `cert_schema` encoder, by rebuilding the
///         same sample certs and checking BOTH the payload bytes and the
///         keccak digest against the golden vectors authored by
///         `cargo test -p chain-core --features keccak --test golden_vectors`.
///         If either side drifts, this test (or the Rust verify_vectors
///         test) fails — the two encoders cannot silently diverge.
contract CertLibTest is Test {
    /// Mirrors chain-core golden_vectors.rs `leg(seed)`.
    function _leg(uint8 seed) internal pure returns (CertLib.Leg memory) {
        bytes32 chainTag = _fill(seed);
        bytes32 contractId = _fill(seed + 1);
        bytes32 player = _fill(seed + 2);
        // session_key: 20-byte fill (seed+3), as an address.
        address sessionKey = address(uint160(bytes20(_fill20(seed + 3))));
        return CertLib.Leg({
            chainTag: chainTag,
            contractId: contractId,
            player: player,
            sessionKey: sessionKey,
            stake: uint128(seed) * 1_000_000,
            tranche: uint128(seed) * 2_000_000
        });
    }

    function _fill(uint8 b) internal pure returns (bytes32 out) {
        for (uint256 i = 0; i < 32; i++) {
            out |= bytes32(uint256(b)) << (8 * i);
        }
    }

    function _fill20(uint8 b) internal pure returns (bytes20 out) {
        uint160 acc;
        for (uint256 i = 0; i < 20; i++) {
            acc |= uint160(b) << uint160(8 * i);
        }
        out = bytes20(acc);
    }

    function _sampleMatchLive() internal pure returns (CertLib.MatchLiveCert memory) {
        return CertLib.MatchLiveCert({
            matchId: _fill(0xAA),
            tournamentId: 7,
            matchupCommitment: _fill(0xBB),
            legA: _leg(0x10),
            legB: _leg(0x20),
            quoteTimestamp: 1_765_000_000,
            quoteMaxAgeSecs: 300,
            matchDeadline: 1_765_000_900,
            claimWindowSecs: 3600,
            aIsP1: 1
        });
    }

    struct Vector {
        bytes payload;
        bytes32 digest;
    }

    function _readVector(string memory key) internal view returns (Vector memory v) {
        string memory json = vm.readFile("../tests/fixtures/cert-vectors.json");
        v.payload = vm.parseJsonBytes(json, string.concat(".", key, ".payload"));
        v.digest = vm.parseJsonBytes32(json, string.concat(".", key, ".digest"));
    }

    function test_matchLive_matches_golden_vector() public view {
        CertLib.MatchLiveCert memory cert = _sampleMatchLive();
        bytes memory payload = CertLib.encodeMatchLive(cert);
        Vector memory v = _readVector("match_live");
        assertEq(payload, v.payload, "match-live payload drift vs Rust encoder");
        assertEq(keccak256(payload), v.digest, "match-live digest drift");
        assertEq(CertLib.matchLiveDigest(cert), v.digest, "matchLiveDigest helper drift");
    }

    function test_checkpoint_matches_golden_vector() public view {
        Vector memory ml = _readVector("match_live");
        CertLib.Checkpoint memory cp = CertLib.Checkpoint({
            matchLiveDigest: ml.digest,
            stepCount: 4,
            p1Commit: _fill(0xC1),
            p2Commit: _fill(0xC2),
            p1Guess: 1,
            p2Guess: 0,
            firstCommitter: 1,
            matchupType: 1,
            transcriptHash: _fill(0xD0),
            rMatchup: _fill(0xE1)
        });
        bytes memory payload = CertLib.encodeCheckpoint(cp);
        Vector memory v = _readVector("checkpoint");
        assertEq(payload, v.payload, "checkpoint payload drift vs Rust encoder");
        assertEq(CertLib.checkpointDigest(cp), v.digest, "checkpoint digest drift");
    }

    function test_outcome_matches_golden_vector() public view {
        Vector memory ml = _readVector("match_live");
        CertLib.OutcomeCert memory oc = CertLib.OutcomeCert({
            matchId: _fill(0xAA),
            matchLiveDigest: ml.digest,
            outcomeKind: CertLib.HETERO_P1_WINS,
            stepCount: 4,
            p1Guess: 1,
            p2Guess: 0,
            firstCommitter: 1,
            matchupType: 1,
            transcriptHash: _fill(0xD0)
        });
        bytes memory payload = CertLib.encodeOutcome(oc);
        Vector memory v = _readVector("outcome");
        assertEq(payload, v.payload, "outcome payload drift vs Rust encoder");
        assertEq(CertLib.outcomeDigest(oc), v.digest, "outcome digest drift");
    }

    function test_magic_constants_match_preimages() public pure {
        assertEq(CertLib.MATCH_LIVE_MAGIC, keccak256("SWARM_XCHAIN_MATCH_LIVE"));
        assertEq(CertLib.CHECKPOINT_MAGIC, keccak256("SWARM_XCHAIN_CHECKPOINT"));
        assertEq(CertLib.OUTCOME_MAGIC, keccak256("SWARM_XCHAIN_OUTCOME"));
    }

    function test_deriveTerminalOutcome_matches_payoff_matrix() public pure {
        // Heterogeneous (matchupType=1): correct guess wins both stakes.
        CertLib.Checkpoint memory cp = _terminal(1, 1, 0, 1); // p1 correct (guess 1 == type 1)
        assertEq(CertLib.deriveTerminalOutcome(cp), CertLib.HETERO_P1_WINS);

        cp = _terminal(1, 0, 1, 2); // p2 correct
        assertEq(CertLib.deriveTerminalOutcome(cp), CertLib.HETERO_P2_WINS);

        cp = _terminal(1, 1, 1, 1); // both correct → first committer (1) wins
        assertEq(CertLib.deriveTerminalOutcome(cp), CertLib.HETERO_P1_WINS);
        cp = _terminal(1, 1, 1, 2); // both correct → first committer (2) wins
        assertEq(CertLib.deriveTerminalOutcome(cp), CertLib.HETERO_P2_WINS);

        cp = _terminal(1, 0, 0, 1); // both wrong
        assertEq(CertLib.deriveTerminalOutcome(cp), CertLib.BOTH_WRONG);

        // Homogeneous (matchupType=0).
        cp = _terminal(0, 0, 0, 1); // both correct (guess 0 == type 0)
        assertEq(CertLib.deriveTerminalOutcome(cp), CertLib.HOMOG_BOTH_CORRECT);
        cp = _terminal(0, 0, 1, 1); // p1 correct only
        assertEq(CertLib.deriveTerminalOutcome(cp), CertLib.HOMOG_P1_CORRECT);
        cp = _terminal(0, 1, 0, 1); // p2 correct only
        assertEq(CertLib.deriveTerminalOutcome(cp), CertLib.HOMOG_P2_CORRECT);
        cp = _terminal(0, 1, 1, 1); // both wrong
        assertEq(CertLib.deriveTerminalOutcome(cp), CertLib.BOTH_WRONG);
    }

    function test_deriveClaimOutcome_timeout_semantics() public pure {
        // step 1: only first committer committed → committer wins.
        CertLib.Checkpoint memory cp = _terminal(1, 255, 255, 1);
        cp.stepCount = 1;
        assertEq(CertLib.deriveClaimOutcome(cp), CertLib.TIMEOUT_P1_WINS);

        // step 3: both committed, only p1 revealed → revealer (p1) wins.
        cp = _terminal(1, 1, 255, 1);
        cp.stepCount = 3;
        assertEq(CertLib.deriveClaimOutcome(cp), CertLib.TIMEOUT_P1_WINS);

        // step 0 / step 2: nobody can be credited → both forfeit.
        cp.stepCount = 0;
        assertEq(CertLib.deriveClaimOutcome(cp), CertLib.TIMEOUT_BOTH_FORFEIT);
        cp.stepCount = 2;
        assertEq(CertLib.deriveClaimOutcome(cp), CertLib.TIMEOUT_BOTH_FORFEIT);
    }

    /// The cross-language derivation pin (M11): chain-core generates
    /// outcome-derivation.json (its `derive_outcome_kind` output over an
    /// exhaustive input set); this asserts Solidity's `deriveClaimOutcome` agrees
    /// on every row, so the Rust and Solidity derivations can never drift.
    function test_deriveClaimOutcome_matches_truth_table() public view {
        string memory json = vm.readFile("../tests/fixtures/outcome-derivation.json");
        uint256[] memory step = vm.parseJsonUintArray(json, ".step_count");
        uint256[] memory p1 = vm.parseJsonUintArray(json, ".p1_guess");
        uint256[] memory p2 = vm.parseJsonUintArray(json, ".p2_guess");
        uint256[] memory fc = vm.parseJsonUintArray(json, ".first_committer");
        uint256[] memory mt = vm.parseJsonUintArray(json, ".matchup_type");
        uint256[] memory kind = vm.parseJsonUintArray(json, ".expected_kind");

        uint256 n = step.length;
        assertTrue(n > 0, "empty truth table");
        assertEq(kind.length, n, "expected_kind length mismatch");

        for (uint256 i = 0; i < n; i++) {
            CertLib.Checkpoint memory cp = CertLib.Checkpoint({
                matchLiveDigest: bytes32(0),
                stepCount: uint8(step[i]),
                p1Commit: bytes32(0),
                p2Commit: bytes32(0),
                p1Guess: uint8(p1[i]),
                p2Guess: uint8(p2[i]),
                firstCommitter: uint8(fc[i]),
                matchupType: uint8(mt[i]),
                transcriptHash: bytes32(0),
                rMatchup: bytes32(0)
            });
            assertEq(
                uint256(CertLib.deriveClaimOutcome(cp)),
                kind[i],
                "Solidity deriveClaimOutcome diverges from the chain-core truth table"
            );
        }
    }

    function _terminal(uint8 matchupType, uint8 p1Guess, uint8 p2Guess, uint8 firstCommitter)
        internal
        pure
        returns (CertLib.Checkpoint memory)
    {
        return CertLib.Checkpoint({
            matchLiveDigest: bytes32(0),
            stepCount: 4,
            p1Commit: bytes32(0),
            p2Commit: bytes32(0),
            p1Guess: p1Guess,
            p2Guess: p2Guess,
            firstCommitter: firstCommitter,
            matchupType: matchupType,
            transcriptHash: bytes32(0),
            rMatchup: bytes32(0)
        });
    }
}
