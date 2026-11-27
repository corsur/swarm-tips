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
    uint256 internal constant OPERATOR_PK = 0x09E7A;
    address internal owner = address(0xA11CE);
    address internal operator;
    address internal treasury = address(0xCAFE);
    address internal mallory = address(0xBAD);
    address internal alice = address(0xA1);
    address internal bob = address(0xB0B);
    uint128 internal constant STAKE = 0.0027 ether;

    function setUp() public {
        operator = vm.addr(OPERATOR_PK);
        CoordinationGameV4 impl = new CoordinationGameV4();
        bytes memory init = abi.encodeCall(
            CoordinationGameV4.initialize, (owner, operator, treasury, 5000, STAKE, 3600, 7200, 1, 365 days)
        );
        game = CoordinationGameV4(payable(address(new ERC1967Proxy(address(impl), init))));
        vm.deal(alice, 10 ether);
        vm.deal(bob, 10 ether);
    }

    function _createSig(bytes32 gameId, address creator, bytes32 matchupCommitment)
        internal
        view
        returns (bytes memory)
    {
        bytes32 digest = keccak256(abi.encode(block.chainid, address(game), gameId, creator, matchupCommitment));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(OPERATOR_PK, digest);
        return abi.encodePacked(r, s, v);
    }

    function _joinSig(bytes32 gameId, address joiner) internal view returns (bytes memory) {
        bytes32 digest = keccak256(abi.encode(block.chainid, address(game), gameId, joiner));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(OPERATOR_PK, digest);
        return abi.encodePacked(r, s, v);
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
        game.joinGame(bytes32(uint256(1)), mallory, "");

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

    // ----- MED-13: gameSeasonId snapshot across season rollover -------------

    function test_midGameSeasonRollover_creditsSnapshottedCreationSeason() public {
        bytes32 gameId = keccak256("season-rollover-reveal");
        bytes32 rMatchup = bytes32(uint256(2)); // LSB = 0 (homogeneous)
        bytes32 matchupCommit = sha256(abi.encodePacked(rMatchup));

        // Created in Season 1.
        vm.prank(alice);
        game.createGame{value: STAKE}(gameId, matchupCommit, _createSig(gameId, alice, matchupCommit), alice);
        assertEq(game.gameSeasonId(gameId), 1, "gameSeasonId snapshotted to season 1 at createGame");

        vm.prank(bob);
        game.joinGame{value: STAKE}(gameId, bob, _joinSig(gameId, bob));

        // Owner rolls over to Season 2 while the game is still in flight.
        vm.prank(owner);
        game.startSeason(2, 365 days);
        assertEq(game.currentSeasonId(), 2, "live currentSeasonId moved to season 2");

        // HOMOG_P1_CORRECT: alice guesses 0 (correct), bob guesses 1 (wrong).
        bytes32 rAlice = bytes32(uint256(4)); // LSB = 0
        bytes32 rBob = bytes32(uint256(5)); // LSB = 1
        bytes32 cAlice = sha256(abi.encodePacked(rAlice));
        bytes32 cBob = sha256(abi.encodePacked(rBob));
        vm.prank(alice);
        game.commitGuess(gameId, cAlice);
        vm.prank(bob);
        game.commitGuess(gameId, cBob);

        vm.prank(alice);
        game.revealGuess(gameId, rAlice, rMatchup);
        vm.prank(bob);
        game.revealGuess(gameId, rBob, bytes32(0));

        // Gain = 2 * STAKE - STAKE/2 = 1.5 * STAKE; 50% split -> 0.75 * STAKE to prize pot.
        uint256 gain = uint256(STAKE) * 2 - uint256(STAKE) / 2;
        uint256 expectedPrize = gain - (gain * 5000 / 10_000);

        (,,,, uint256 s1Accrued,,) = game.seasons(1);
        (,,,, uint256 s2Accrued,,) = game.seasons(2);
        assertEq(s1Accrued, expectedPrize, "forfeit accrued to snapshotted season 1");
        assertEq(s2Accrued, 0, "season 2 untouched by season-1 game");

        (uint64 s1AliceWins, uint64 s1AliceGames,) = game.records(1, alice);
        (uint64 s1BobWins, uint64 s1BobGames,) = game.records(1, bob);
        (uint64 s2AliceWins, uint64 s2AliceGames,) = game.records(2, alice);
        assertEq(s1AliceWins, 1, "alice win recorded in season 1");
        assertEq(s1AliceGames, 1, "alice game recorded in season 1");
        assertEq(s1BobWins, 0, "bob loss recorded in season 1");
        assertEq(s1BobGames, 1, "bob game recorded in season 1");
        assertEq(s2AliceWins, 0, "season 2 wins untouched");
        assertEq(s2AliceGames, 0, "season 2 games untouched");
    }

    function test_midGameSeasonRollover_timeoutResolutionCreditsSnapshottedSeason() public {
        bytes32 gameId = keccak256("season-rollover-timeout");
        bytes32 matchupCommit = sha256(abi.encodePacked(bytes32(uint256(2))));

        vm.prank(alice);
        game.createGame{value: STAKE}(gameId, matchupCommit, _createSig(gameId, alice, matchupCommit), alice);
        vm.prank(bob);
        game.joinGame{value: STAKE}(gameId, bob, _joinSig(gameId, bob));

        // Rollover to Season 2 before timeout fires.
        vm.prank(owner);
        game.startSeason(2, 365 days);

        // Neither player commits -> TIMEOUT_BOTH_FORFEIT (gain = 2 * STAKE).
        vm.warp(block.timestamp + 3600);
        game.resolveTimeout(gameId);

        uint256 gain = uint256(STAKE) * 2;
        uint256 expectedPrize = gain - (gain * 5000 / 10_000);
        (,,,, uint256 s1Accrued,,) = game.seasons(1);
        (,,,, uint256 s2Accrued,,) = game.seasons(2);
        assertEq(s1Accrued, expectedPrize, "timeout forfeit credited to season 1");
        assertEq(s2Accrued, 0, "season 2 untouched");
    }

    // ----- P1-04, EVM-11, P2-04, P2-05, P2-07, P2-08, EVM-10 ----------------

    function test_P1_04_joinGame_revertsAtOrAfterCommitWindow() public {
        bytes32 gameId = keccak256("v4-expired-join");
        bytes32 mc = sha256(abi.encodePacked(bytes32(uint256(2))));
        vm.prank(alice);
        game.createGame{value: STAKE}(gameId, mc, _createSig(gameId, alice, mc), alice);

        vm.warp(block.timestamp + 3600);
        bytes memory joinSig = _joinSig(gameId, bob);
        vm.prank(bob);
        vm.expectRevert(CoordinationGameV4.DeadlinePassed.selector);
        game.joinGame{value: STAKE}(gameId, bob, joinSig);
    }

    function test_EVM11_copycatCommitReverts_and_secondRevealerValidRMatchupSucceeds() public {
        bytes32 gameId = keccak256("v4-evm11");
        bytes32 rMatchup = bytes32(uint256(3)); // LSB = 1 (hetero)
        bytes32 mc = sha256(abi.encodePacked(rMatchup));
        vm.prank(alice);
        game.createGame{value: STAKE}(gameId, mc, _createSig(gameId, alice, mc), alice);
        vm.prank(bob);
        game.joinGame{value: STAKE}(gameId, bob, _joinSig(gameId, bob));

        bytes32 rAlice = bytes32(uint256(5));
        bytes32 rBob = bytes32(uint256(7));
        bytes32 cAlice = sha256(abi.encodePacked(rAlice));
        bytes32 cBob = sha256(abi.encodePacked(rBob));

        vm.prank(alice);
        game.commitGuess(gameId, cAlice);

        // Copycat commitment rejected.
        vm.prank(bob);
        vm.expectRevert(CoordinationGameV4.BadCommitment.selector);
        game.commitGuess(gameId, cAlice);

        vm.prank(bob);
        game.commitGuess(gameId, cBob);

        vm.prank(alice);
        game.revealGuess(gameId, rAlice, rMatchup);

        // Wrong non-null rMatchup still rejected.
        vm.prank(bob);
        vm.expectRevert(CoordinationGameV4.CertMismatch.selector);
        game.revealGuess(gameId, rBob, keccak256("bad-matchup"));

        // Valid rMatchup from second revealer accepted (concurrent reveal safe).
        vm.prank(bob);
        game.revealGuess(gameId, rBob, rMatchup);
        (CoordinationGameV4.Status st,,,,,,,,,,,,,,,,) = game.games(gameId);
        assertEq(uint8(st), uint8(CoordinationGameV4.Status.Resolved));
    }

    function test_P2_04_sessionTtlCappedAtMaxSessionTtl() public {
        address sk = address(0x5E55);
        uint64 maxExpiry = uint64(block.timestamp) + game.MAX_SESSION_TTL();

        vm.prank(alice);
        vm.expectRevert(CoordinationGameV4.BadSession.selector);
        game.openSession(sk, maxExpiry + 1);

        vm.prank(alice);
        vm.expectRevert(CoordinationGameV4.BadSession.selector);
        game.openSessionAndDeposit{value: STAKE}(sk, maxExpiry + 1, 0);

        vm.prank(alice);
        game.openSession(sk, maxExpiry);
        (address savedKey, uint64 savedExp) = game.sessions(alice);
        assertEq(savedKey, sk);
        assertEq(savedExp, maxExpiry);
    }

    function test_P2_05_claimPrize_pushesDirectlyToWalletNotWithdrawable() public {
        // Play 5 homogeneous both-wrong games in Season 1 so alice is eligible & pot has funds.
        bytes32 rMatchup = bytes32(uint256(2)); // LSB = 0 (homog)
        bytes32 mc = sha256(abi.encodePacked(rMatchup));
        for (uint256 i = 0; i < 5; i++) {
            bytes32 gid = keccak256(abi.encode("p2-05", i));
            vm.prank(alice);
            game.createGame{value: STAKE}(gid, mc, _createSig(gid, alice, mc), alice);
            vm.prank(bob);
            game.joinGame{value: STAKE}(gid, bob, _joinSig(gid, bob));
            bytes32 rA = bytes32((i + 1) * 2 + 1); // LSB = 1 (wrong for homog)
            bytes32 rB = bytes32((i + 100) * 2 + 1); // LSB = 1 (wrong for homog)
            bytes32 cA = sha256(abi.encodePacked(rA));
            bytes32 cB = sha256(abi.encodePacked(rB));
            vm.prank(alice);
            game.commitGuess(gid, cA);
            vm.prank(bob);
            game.commitGuess(gid, cB);
            vm.prank(alice);
            game.revealGuess(gid, rA, rMatchup);
            vm.prank(bob);
            game.revealGuess(gid, rB, bytes32(0));
        }

        (,,,, uint256 accrued,,) = game.seasons(1);
        assertGt(accrued, 0);
        bytes32 leaf = game.leafFor(alice, accrued);

        vm.warp(block.timestamp + 366 days);
        vm.prank(owner);
        game.finalizeSeason(1, leaf, accrued);

        uint256 walletBefore = alice.balance;
        bytes32[] memory proof = new bytes32[](0);
        vm.prank(alice);
        game.claimPrize(1, accrued, proof);

        assertEq(alice.balance, walletBefore + accrued, "season prize pushed directly to wallet");
        assertEq(game.withdrawable(alice), 0, "prize not left in hot-session-spendable withdrawable ledger");
    }

    function test_P2_07_finalizedSeasonSpilloverRoutesToActiveCurrentSeason() public {
        bytes32 gameId = keccak256("v4-finalized-spillover");
        bytes32 mc = sha256(abi.encodePacked(bytes32(uint256(2))));
        vm.prank(alice);
        game.createGame{value: STAKE}(gameId, mc, _createSig(gameId, alice, mc), alice);
        vm.prank(bob);
        game.joinGame{value: STAKE}(gameId, bob, _joinSig(gameId, bob));

        // Season 1 expires and finalizes while Season 2 is started and open.
        vm.warp(block.timestamp + 366 days);
        vm.prank(owner);
        game.finalizeSeason(1, bytes32(uint256(1)), 0);
        vm.prank(owner);
        game.startSeason(2, 365 days);

        // Resolving the Season-1 game after Season 1 is finalized routes prize + records to open Season 2.
        game.resolveTimeout(gameId);

        uint256 gain = uint256(STAKE) * 2;
        uint256 expectedPrize = gain - (gain * 5000 / 10_000);
        (,,,, uint256 s1Accrued,,) = game.seasons(1);
        (,,,, uint256 s2Accrued,,) = game.seasons(2);
        assertEq(s1Accrued, 0, "finalized season 1 untouched");
        assertEq(s2Accrued, expectedPrize, "prize routed to active season 2");

        (, uint64 s1AliceGames,) = game.records(1, alice);
        (, uint64 s2AliceGames,) = game.records(2, alice);
        assertEq(s1AliceGames, 0, "finalized season 1 record untouched");
        assertEq(s2AliceGames, 1, "record attributed to active season 2");
    }

    function test_P2_08_resolveStrictCei_recordsUpdatedBeforeSettleExternalCall() public {
        CeiObserverPlayer observer = new CeiObserverPlayer(game);
        vm.deal(address(observer), 10 ether);

        bytes32 gameId = keccak256("v4-cei-check");
        bytes32 rMatchup = bytes32(uint256(3)); // LSB = 1 (hetero)
        bytes32 mc = sha256(abi.encodePacked(rMatchup));

        observer.create(gameId, mc, _createSig(gameId, address(observer), mc), STAKE);
        vm.prank(bob);
        game.joinGame{value: STAKE}(gameId, bob, _joinSig(gameId, bob));

        bytes32 rObs = bytes32(uint256(5)); // LSB = 1 (correct)
        bytes32 rBob = bytes32(uint256(4)); // LSB = 0 (wrong)
        bytes32 cObs = sha256(abi.encodePacked(rObs));
        bytes32 cBob = sha256(abi.encodePacked(rBob));
        observer.commit(gameId, cObs);
        vm.prank(bob);
        game.commitGuess(gameId, cBob);

        observer.reveal(gameId, rObs, rMatchup);
        vm.prank(bob);
        game.revealGuess(gameId, rBob, bytes32(0));

        assertTrue(observer.receivedCallback(), "observer receive() executed during _settle");
        assertEq(observer.observedWinsDuringReceive(), 1, "wins MUST already be updated before _settle external call");
        assertEq(observer.observedGamesDuringReceive(), 1, "games MUST already be updated before _settle external call");
    }

    function test_EVM10_governanceHardening() public {
        // initialize rejects address(0) owner.
        CoordinationGameV4 impl2 = new CoordinationGameV4();
        bytes memory badInit = abi.encodeCall(
            CoordinationGameV4.initialize, (address(0), operator, treasury, 5000, STAKE, 3600, 7200, 1, 365 days)
        );
        vm.expectRevert(CoordinationGameV4.BadConfig.selector);
        new ERC1967Proxy(address(impl2), badInit);

        // renounceOwnership reverts on UUPS proxy.
        vm.prank(owner);
        vm.expectRevert(CoordinationGameV4.BadConfig.selector);
        game.renounceOwnership();
        assertEq(game.owner(), owner);

        // acceptOwnership rejects operatorSigner (key separation).
        vm.prank(owner);
        game.transferOwnership(operator);
        vm.prank(operator);
        vm.expectRevert(CoordinationGameV4.BadConfig.selector);
        game.acceptOwnership();
        assertEq(game.owner(), owner);

        // sweepUnclaimed rejects address(0) recipient.
        vm.warp(block.timestamp + 366 days);
        vm.prank(owner);
        game.finalizeSeason(1, bytes32(uint256(1)), 0);
        vm.warp(block.timestamp + 91 days);
        vm.prank(owner);
        vm.expectRevert(CoordinationGameV4.BadConfig.selector);
        game.sweepUnclaimed(1, address(0));
    }
}

/// Helper contract that observes `records` inside `receive()` during `_settle`
/// to prove strict CEI ordering in `CoordinationGameV4._resolve` (P2-08).
contract CeiObserverPlayer {
    CoordinationGameV4 internal immutable game;
    bool public receivedCallback;
    uint64 public observedWinsDuringReceive;
    uint64 public observedGamesDuringReceive;

    constructor(CoordinationGameV4 game_) {
        game = game_;
    }

    function create(bytes32 gameId, bytes32 mc, bytes calldata sig, uint256 stake) external {
        game.createGame{value: stake}(gameId, mc, sig, address(this));
    }

    function commit(bytes32 gameId, bytes32 c) external {
        game.commitGuess(gameId, c);
    }

    function reveal(bytes32 gameId, bytes32 r, bytes32 rMatchup) external {
        game.revealGuess(gameId, r, rMatchup);
    }

    receive() external payable {
        receivedCallback = true;
        (observedWinsDuringReceive, observedGamesDuringReceive,) = game.records(1, address(this));
    }
}
