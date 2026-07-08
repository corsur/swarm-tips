// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";

/// @notice Cross-impl golden-vector parity for the Shillbot payout matrix.
///         Reads the SAME tests/fixtures/task-payout-vectors.json that the TS
///         oracle (sdk/task-outcome-oracle.ts) generates and the Rust program
///         test (scoring.rs::payout_vectors_match_compute_payment) verifies.
///
///         This re-derives payment/fee AND the full outcome distribution with
///         an INDEPENDENT Solidity implementation of the formula and asserts it
///         matches the fixture — so a bug in the TS oracle's integer arithmetic
///         or outcome split fails here (different language, different int
///         semantics). The separate testFuzz_kind0Payment_matchesReference in
///         ShillbotEscrowInvariant.t.sol proves the deployed contract's private
///         _computePayment equals this same reference formula, completing the
///         transitive pin: fixture == reference == on-chain _computePayment.
contract ShillbotEscrowVectorsTest is Test {
    string internal constant FIXTURE = "../tests/fixtures/task-payout-vectors.json";
    uint256 internal constant MAX_SCORE = 1_000_000;
    uint256 internal constant BPS_DENOM = 10_000;

    function _ref(uint256 score, uint256 threshold, uint256 escrow, uint256 feeBps)
        internal
        pure
        returns (uint256 payment, uint256 fee, uint256 remainder)
    {
        if (score < threshold || threshold >= MAX_SCORE) {
            return (0, 0, escrow);
        }
        uint256 range = MAX_SCORE - threshold;
        uint256 gross = (escrow * (score - threshold)) / range;
        fee = (gross * feeBps) / BPS_DENOM;
        payment = gross - fee;
        remainder = escrow - payment - fee;
    }

    function test_payoutVectors_matchReferenceDerivation() public view {
        string memory json = vm.readFile(FIXTURE);
        uint256 count = vm.parseJsonUint(json, ".count");
        assertGt(count, 0, "fixture has no vectors");

        for (uint256 i = 0; i < count; i++) {
            _checkVector(json, i);
        }
    }

    function _checkVector(string memory json, uint256 i) internal pure {
        string memory base = string.concat(".vectors[", vm.toString(i), "]");
        string memory sc = string.concat(base, ".scenario");

        uint256 escrow = vm.parseUint(vm.parseJsonString(json, string.concat(sc, ".escrowLamports")));
        uint256 threshold = vm.parseJsonUint(json, string.concat(sc, ".qualityThreshold"));
        uint256 feeBps = vm.parseJsonUint(json, string.concat(sc, ".protocolFeeBps"));
        uint256 score = vm.parseJsonUint(json, string.concat(sc, ".compositeScore"));
        uint256 mult = vm.parseJsonUint(json, string.concat(sc, ".challengeBondMultiplier"));
        uint256 slashBps = vm.parseJsonUint(json, string.concat(sc, ".bondSlashTreasuryBps"));
        uint256 outcome = vm.parseJsonUint(json, string.concat(sc, ".outcome"));

        (uint256 payment, uint256 fee, uint256 remainder) = _ref(score, threshold, escrow, feeBps);
        assertEq(payment, vm.parseUint(vm.parseJsonString(json, string.concat(base, ".payment"))), "payment");
        assertEq(fee, vm.parseUint(vm.parseJsonString(json, string.concat(base, ".fee"))), "fee");
        assertEq(remainder, vm.parseUint(vm.parseJsonString(json, string.concat(base, ".remainder"))), "remainder");

        uint256 bond = escrow * mult;
        assertEq(bond, vm.parseUint(vm.parseJsonString(json, string.concat(base, ".bond"))), "bond");

        (uint256 agent, uint256 treasury, uint256 client, uint256 challenger) =
            _distribute(outcome, escrow, bond, payment, fee, remainder, slashBps);

        string memory po = string.concat(base, ".payout");
        assertEq(agent, vm.parseUint(vm.parseJsonString(json, string.concat(po, ".agentLamports"))), "agent");
        assertEq(treasury, vm.parseUint(vm.parseJsonString(json, string.concat(po, ".treasuryLamports"))), "treasury");
        assertEq(client, vm.parseUint(vm.parseJsonString(json, string.concat(po, ".clientLamports"))), "client");
        assertEq(
            challenger,
            vm.parseUint(vm.parseJsonString(json, string.concat(po, ".challengerLamports"))),
            "challenger"
        );
    }

    /// Mirror of deriveTaskOutcome's terminal split. Outcome codes:
    /// 0 Finalized, 1 ResolvedAgentWins, 2 ResolvedChallengerWins,
    /// 3 DefaultResolved, 4 Expired.
    function _distribute(
        uint256 outcome,
        uint256 escrow,
        uint256 bond,
        uint256 payment,
        uint256 fee,
        uint256 remainder,
        uint256 slashBps
    ) internal pure returns (uint256 agent, uint256 treasury, uint256 client, uint256 challenger) {
        if (outcome == 0) {
            return (payment, fee, remainder, 0);
        }
        if (outcome == 1) {
            uint256 toTreasury = (bond * slashBps) / BPS_DENOM;
            return (payment + (bond - toTreasury), fee + toTreasury, remainder, 0);
        }
        if (outcome == 2) {
            return (0, 0, escrow, bond);
        }
        if (outcome == 3) {
            return (payment, fee, remainder, bond);
        }
        if (outcome == 4) {
            return (0, 0, escrow, 0);
        }
        revert("unknown outcome");
    }
}
