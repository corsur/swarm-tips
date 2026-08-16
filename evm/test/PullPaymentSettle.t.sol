// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {PullPayment} from "../src/base/PullPayment.sol";

/// Minimal concrete PullPayment to exercise the shared `_settle` helper in
/// isolation — the push-preferred payout that every game contract's resolution
/// path now uses (CoordinationGameV4._resolve, CrossChainGame.settle). Proven
/// here once, at the foundation, so the game suites only need to prove the
/// WIRING (that resolve/settle call it), not the push/fallback mechanics.
contract PullPaymentHarness is PullPayment {
    function settle(address to, uint256 amount) external {
        _settle(to, amount);
    }

    receive() external payable {}
}

/// Reverts on receive — the classic recipient that a naive push would let brick
/// a state transition. `_settle` must fall back to the ledger instead.
contract RevertOnReceive {
    receive() external payable {
        revert("nope");
    }
}

/// Accepts ether but burns far past the `_settle` gas cap, so the push runs out
/// of the forwarded gas and fails — must also fall back to the ledger.
contract GasGuzzler {
    uint256[] private junk;

    receive() external payable {
        for (uint256 i = 0; i < 1000; i++) {
            junk.push(i);
        }
    }
}

contract PullPaymentSettleTest is Test {
    PullPaymentHarness internal pp;

    event Withdrawn(address indexed to, uint256 amount);

    function setUp() public {
        pp = new PullPaymentHarness();
        vm.deal(address(pp), 100 ether);
    }

    function test_settlePushesToAnEOAImmediately() public {
        address payable alice = payable(makeAddr("alice"));
        uint256 before = alice.balance;

        vm.expectEmit(true, false, false, true, address(pp));
        emit Withdrawn(alice, 1 ether);
        pp.settle(alice, 1 ether);

        assertEq(alice.balance, before + 1 ether, "EOA paid immediately by push");
        assertEq(pp.withdrawable(alice), 0, "nothing parked in the ledger");
    }

    function test_settleZeroIsANoop() public {
        address alice = makeAddr("alice");
        pp.settle(alice, 0);
        assertEq(pp.withdrawable(alice), 0);
        assertEq(address(pp).balance, 100 ether, "no ether moved");
    }

    function test_settleFallsBackToLedgerWhenRecipientReverts() public {
        RevertOnReceive hostile = new RevertOnReceive();
        uint256 potBefore = address(pp).balance;

        pp.settle(address(hostile), 1 ether);

        assertEq(address(hostile).balance, 0, "push to a reverting recipient did not force through");
        assertEq(pp.withdrawable(address(hostile)), 1 ether, "payout fell back to the pull ledger");
        assertEq(address(pp).balance, potBefore, "funds stay in the contract, claimable later");
    }

    function test_settleFallsBackWhenRecipientBurnsPastTheGasCap() public {
        GasGuzzler hog = new GasGuzzler();
        pp.settle(address(hog), 1 ether);
        assertEq(pp.withdrawable(address(hog)), 1 ether, "a gas-heavy recipient falls back to the ledger");
        assertEq(address(hog).balance, 0);
    }

    /// The M1 guarantee end-to-end: a recipient that reverts forever can never
    /// take money it isn't owed and can never block others — its credit sits in
    /// the ledger, and only its OWN pull fails.
    function test_ledgerFallbackKeepsFundsAndOnlyFailsTheHostilesOwnPull() public {
        RevertOnReceive hostile = new RevertOnReceive();
        pp.settle(address(hostile), 1 ether);

        vm.expectRevert("withdraw failed");
        pp.withdrawFor(address(hostile));

        assertEq(pp.withdrawable(address(hostile)), 1 ether, "credit intact after a failed pull");
    }
}
