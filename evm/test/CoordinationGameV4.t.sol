// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CoordinationGameV4} from "../src/CoordinationGameV4.sol";
import {SeasonPot} from "../src/SeasonPot.sol";
import {ERC1967Proxy} from "../lib/openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";

/// A trivial "next version" used only to prove an upgrade preserves state.
contract V5Probe is CoordinationGameV4 {
    function versionTag() external pure returns (string memory) {
        return "v5";
    }
}

contract CoordinationGameV4Test is Test {
    CoordinationGameV4 internal game;
    address internal owner = address(0xA11CE);
    address internal operator = address(0x09E7A);
    address internal treasury = address(0xCAFE);
    address internal mallory = address(0xBAD);

    function setUp() public {
        CoordinationGameV4 impl = new CoordinationGameV4();
        bytes memory init = abi.encodeCall(
            CoordinationGameV4.initialize, (owner, operator, treasury, 5000, 0.0027 ether, 3600, 7200, 1, 365 days)
        );
        game = CoordinationGameV4(payable(address(new ERC1967Proxy(address(impl), init))));
    }

    // ----- proxy wiring ----------------------------------------------------

    function test_initializeSetsStateOnTheProxy_andSeasonOne() public view {
        assertEq(game.owner(), owner, "owner lives in PROXY storage");
        assertEq(game.stakeWei(), 0.0027 ether);
        assertEq(game.currentSeasonId(), 1, "season 1 opened at init");
        (uint64 start, uint64 end,,,,,) = game.seasons(1);
        assertEq(end - start, 365 days);
    }

    function test_initializeCannotBeRerun() public {
        vm.expectRevert();
        game.initialize(mallory, operator, treasury, 5000, 0.0027 ether, 3600, 7200, 2, 365 days);
    }

    /// An uninitialized implementation is a classic UUPS takeover route.
    function test_implementationItselfIsLocked() public {
        CoordinationGameV4 impl = new CoordinationGameV4();
        vm.expectRevert();
        impl.initialize(mallory, operator, treasury, 5000, 0.0027 ether, 3600, 7200, 1, 365 days);
    }

    /// The single most dangerous function on the contract: it can replace all
    /// logic and therefore reach every staked wei.
    function test_onlyOwnerCanUpgrade() public {
        address v5 = address(new V5Probe());
        vm.prank(mallory);
        vm.expectRevert();
        game.upgradeToAndCall(v5, "");
    }

    function test_upgradePreservesState() public {
        // Dirty some state first, across BOTH the game and the season base.
        vm.prank(owner);
        game.startSeason(2, 365 days);
        assertEq(game.currentSeasonId(), 2);

        address v5 = address(new V5Probe());
        vm.prank(owner);
        game.upgradeToAndCall(v5, "");

        assertEq(V5Probe(payable(address(game))).versionTag(), "v5", "logic swapped");
        assertEq(game.currentSeasonId(), 2, "season survived the upgrade");
        assertEq(game.owner(), owner, "owner survived");
        assertEq(game.stakeWei(), 0.0027 ether, "config survived");
        (uint64 s2start,,,,,,) = game.seasons(2);
        assertGt(s2start, 0, "season 2 record survived");
    }

    // ----- one pot, not two ------------------------------------------------

    /// THE REGRESSION THIS GUARDS.
    ///
    /// v4 was first scaffolded by copying v3, which brought along
    /// `prizePoolWei` + `withdrawPrizePool` — an owner-drainable pot with no
    /// claim path. With SeasonPot added, the contract briefly had TWO pots:
    /// forfeits flowed into the old one, so `claimPrize` had nothing to pay,
    /// and `finalizeSeason` bounded its promise on `address(this).balance`
    /// without subtracting the owner-drainable balance — so a season could
    /// promise money the owner then withdrew.
    ///
    /// There must be exactly one pot, and forfeits must land in it.
    function test_forfeitsAccrueToTheSeason_andThereIsNoOwnerDrain() public {
        // The old escape hatch must not exist on the ABI at all.
        (bool ok,) = address(game).call(abi.encodeWithSignature("withdrawPrizePool(address,uint256)", owner, 1));
        assertFalse(ok, "withdrawPrizePool must be gone");

        (bool ok2,) = address(game).call(abi.encodeWithSignature("prizePoolWei()"));
        assertFalse(ok2, "the second pot must be gone");

        // And a season cannot promise money it never took in.
        vm.warp(block.timestamp + 366 days);
        vm.prank(owner);
        vm.expectRevert(SeasonPot.PromiseExceedsBalance.selector);
        game.finalizeSeason(1, bytes32(uint256(1)), 1 wei);
    }

    /// Pausing must stop NEW games, never withhold money already earned.
    /// A pausable claim would make the pot the owner's to release after all.
    function test_pauseCannotWithholdAnEarnedClaim() public {
        vm.prank(owner);
        game.pause();

        // New games are blocked...
        vm.expectRevert();
        game.joinGame(bytes32(uint256(1)), mallory);

        // ...but the claim path still executes and fails on its OWN guard
        // (nothing to claim), not on the pause.
        vm.warp(block.timestamp + 366 days);
        vm.prank(owner);
        game.finalizeSeason(1, bytes32(uint256(1)), 0);
        bytes32[] memory proof = new bytes32[](0);
        vm.prank(mallory);
        vm.expectRevert(SeasonPot.NothingToClaim.selector);
        game.claimPrize(1, 0, proof);
    }

    // ----- season guards ---------------------------------------------------

    function test_onlyOwnerCanStartOrFinalizeSeasons() public {
        vm.prank(mallory);
        vm.expectRevert();
        game.startSeason(9, 365 days);

        vm.prank(mallory);
        vm.expectRevert();
        game.finalizeSeason(1, bytes32(0), 0);
    }

    /// A season must EXPIRE before it can pay out.
    function test_finalizeRevertsWhileSeasonIsLive() public {
        vm.prank(owner);
        vm.expectRevert(SeasonPot.SeasonStillOpen.selector);
        game.finalizeSeason(1, bytes32(uint256(1)), 0);
    }

    /// Claiming is permissionless by design: the pot is not the owner's to
    /// hand out. It must still reject a caller with no entitlement.
    function test_claimIsPermissionlessButStillGated() public {
        vm.warp(block.timestamp + 366 days);
        vm.prank(owner);
        game.finalizeSeason(1, bytes32(uint256(1)), 0);

        bytes32[] memory proof = new bytes32[](0);
        vm.prank(mallory);
        vm.expectRevert(SeasonPot.NothingToClaim.selector);
        game.claimPrize(1, 0, proof);
    }
}
