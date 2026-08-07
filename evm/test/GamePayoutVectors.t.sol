// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CoordinationGame} from "../src/CoordinationGame.sol";
import {CoordinationGameV4} from "../src/CoordinationGameV4.sol";

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

/// Exposes v4's win mapping, which is the OTHER half of the shared core and
/// cannot be inferred from the amounts.
contract WinsProbe is CoordinationGameV4 {
    function wins(uint8 kind) external pure returns (bool, bool) {
        return _winsFor(kind);
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

    /// The eligibility gate must be the SAME number on both chains. It was
    /// pinned by a code comment only ("Mirrors chain_core::game::
    /// MIN_GAMES_FOR_PAYOUT") — the same agrees-by-inspection state that let
    /// the OZ merkle format diverge unnoticed.
    function test_minGamesGate_matchesChainCore() public {
        WinsProbe wp = new WinsProbe();
        string memory json = vm.readFile(FIXTURE);
        uint256 want = vm.parseJsonUint(json, ".constants.minGamesForPayout");
        assertEq(uint256(wp.MIN_GAMES_FOR_PAYOUT()), want, "eligibility gate diverges");
    }

    /// `outcome_to_wins` parity. This is separate from the amounts on purpose:
    /// HOMOG_BOTH_CORRECT returns each player their own stake (zero net gain)
    /// and still awards BOTH a win, so a Solidity mirror that derived wins from
    /// the payout would be wrong in exactly that case and right everywhere else.
    function test_winMapping_matchesChainCoreVectors() public {
        WinsProbe wp = new WinsProbe();
        string memory json = vm.readFile(FIXTURE);
        uint256 count = vm.parseJsonUint(json, ".count");
        for (uint256 i = 0; i < count; i++) {
            string memory b = string.concat(".vectors[", vm.toString(i), "]");
            uint8 kind = uint8(vm.parseJsonUint(json, string.concat(b, ".kind")));
            bool wantP1 = vm.parseJsonBool(json, string.concat(b, ".p1Won"));
            bool wantP2 = vm.parseJsonBool(json, string.concat(b, ".p2Won"));
            (bool gotP1, bool gotP2) = wp.wins(kind);
            assertEq(gotP1, wantP1, string.concat("p1Won diverges at vector ", vm.toString(i)));
            assertEq(gotP2, wantP2, string.concat("p2Won diverges at vector ", vm.toString(i)));
        }
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
