// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";

/// @notice Shared deadline-boundary battery (generalization win 7): every
///         timed gate must be probed at deadline−1 / deadline / deadline+1,
///         because the strict-vs-inclusive inequality at the boundary second
///         is exactly where Solana/EVM parity bugs hide. Each helper runs the
///         probe under a state snapshot so the three attempts are independent.
///
///         `action` is an internal callback that makes EXACTLY ONE external
///         call to the contract under test (vm.expectRevert binds to the next
///         non-cheatcode call; cheatcodes like vm.prank inside the callback
///         are fine).
abstract contract BoundaryBattery is Test {
    /// Gate of the form `t < deadline` (e.g. challengeTask): live at
    /// deadline−1, dead at deadline (the dead second) and after.
    function assertLiveStrictlyBefore(uint256 deadline, bytes4 deadError, function() internal action) internal {
        uint256 snap = vm.snapshotState();
        vm.warp(deadline - 1);
        action();
        assertTrue(vm.revertToState(snap), "snapshot revert failed");
        vm.warp(deadline);
        vm.expectRevert(deadError);
        action();
        vm.warp(deadline + 1);
        vm.expectRevert(deadError);
        action();
        assertTrue(vm.revertToState(snap), "snapshot revert failed");
    }

    /// Gate of the form `t <= deadline` (e.g. resolveChallenge): live at
    /// deadline−1 AND at the deadline second, dead strictly after.
    function assertLiveThrough(uint256 deadline, bytes4 deadError, function() internal action) internal {
        uint256 snap = vm.snapshotState();
        vm.warp(deadline - 1);
        action();
        assertTrue(vm.revertToState(snap), "snapshot revert failed");
        vm.warp(deadline);
        action();
        assertTrue(vm.revertToState(snap), "snapshot revert failed");
        vm.warp(deadline + 1);
        vm.expectRevert(deadError);
        action();
        assertTrue(vm.revertToState(snap), "snapshot revert failed");
    }

    /// Gate of the form `t > deadline` (e.g. finalizeTask, defaultResolve,
    /// expireTask): dead at deadline−1 and at the deadline second, live
    /// strictly after.
    function assertLiveStrictlyAfter(uint256 deadline, bytes4 earlyError, function() internal action) internal {
        uint256 snap = vm.snapshotState();
        vm.warp(deadline - 1);
        vm.expectRevert(earlyError);
        action();
        vm.warp(deadline);
        vm.expectRevert(earlyError);
        action();
        vm.warp(deadline + 1);
        action();
        assertTrue(vm.revertToState(snap), "snapshot revert failed");
    }
}
