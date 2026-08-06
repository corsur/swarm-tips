// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CoordinationGame} from "../src/CoordinationGame.sol";

/// Exposes the REAL `_amounts` rather than reimplementing it.
///
/// Deliberately a subclass, not a reference copy: a hand-written mirror can
/// drift from the contract it is supposed to represent, and drift is the exact
/// failure this file exists to catch.
contract PayoutProbe is CoordinationGame {
    constructor(address owner_, address operator_, address treasury_)
        CoordinationGame(owner_, operator_, treasury_, 5000, 0.0027 ether, 3600, 7200)
    {}

    function amounts(uint8 kind, uint256 stake) external pure returns (uint256, uint256, uint256) {
        return _amounts(kind, stake);
    }
}

/// @notice Cross-implementation parity for the Coordination Game payoff matrix.
///
/// `chain_core::game::amounts_for_kind` is the source of truth; this asserts
/// `CoordinationGame._amounts` reproduces it for every vector.
///
/// The cross-chain outcome DERIVATION was already pinned across languages by
/// `outcome-derivation.json`. The same-chain MONEY SPLIT never was — Rust and
/// Solidity each decided who gets paid and nothing compared them, so the two
/// could disagree about a real payout with every test on both sides passing.
/// Same mechanism Shillbot already uses (`ShillbotEscrowVectors.t.sol`).
contract GamePayoutVectorsTest is Test {
    string internal constant FIXTURE = "../tests/fixtures/game-payout-vectors.json";
    PayoutProbe internal probe;

    function setUp() public {
        probe = new PayoutProbe(address(0xA11CE), address(0xB0B), address(0xCAFE));
    }

    function test_payoutMatrix_matchesChainCoreVectors() public view {
        string memory json = vm.readFile(FIXTURE);
        uint256 count = vm.parseJsonUint(json, ".count");
        assertGt(count, 0, "fixture has no vectors");

        for (uint256 i = 0; i < count; i++) {
            string memory b = string.concat(".vectors[", vm.toString(i), "]");
            uint8 kind = uint8(vm.parseJsonUint(json, string.concat(b, ".kind")));
            // Amounts are decimal STRINGS: EVM stakes exceed u64 in wei terms.
            uint256 stake = vm.parseUint(vm.parseJsonString(json, string.concat(b, ".stake")));
            uint256 wantP1 = vm.parseUint(vm.parseJsonString(json, string.concat(b, ".p1")));
            uint256 wantP2 = vm.parseUint(vm.parseJsonString(json, string.concat(b, ".p2")));
            uint256 wantGain = vm.parseUint(vm.parseJsonString(json, string.concat(b, ".gain")));

            (uint256 gotP1, uint256 gotP2, uint256 gotGain) = probe.amounts(kind, stake);

            assertEq(gotP1, wantP1, string.concat("p1 diverges at vector ", vm.toString(i)));
            assertEq(gotP2, wantP2, string.concat("p2 diverges at vector ", vm.toString(i)));
            assertEq(gotGain, wantGain, string.concat("gain diverges at vector ", vm.toString(i)));
            // Conservation, restated on the Solidity side so a bad fixture
            // cannot bless a bad contract.
            assertEq(gotP1 + gotP2 + gotGain, stake * 2, "conservation");
        }
    }
}
