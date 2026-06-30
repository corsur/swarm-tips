// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CoordinationGame} from "../src/CoordinationGame.sol";
import {CertLib} from "../src/CertLib.sol";

/// @notice Drives the same-chain commit-reveal game through every distinct
///         payoff cell and timeout path, asserting the exact pot split against
///         the canonical matrix (test EOAs pay no gas, so balances are exact).
///         The outcome kinds come from CertLib, the same derivation the Solana
///         program and the cross-chain cert use — so these expectations are the
///         same matrix proven combinatorially in tests/coordination-game.ts.
contract CoordinationGameTest is Test {
    CoordinationGame internal game;

    uint256 internal constant operatorPk = 0xA11CE;
    address internal operator;
    address internal owner = address(0xB0B);
    address internal treasury = address(0x7EA5);
    address internal p1 = address(0x9001);
    address internal p2 = address(0x9002);

    uint128 internal constant STAKE = 0.05 ether;
    uint16 internal constant SPLIT_BPS = 5000;
    uint32 internal constant COMMIT_TIMEOUT = 3600;
    uint32 internal constant REVEAL_TIMEOUT = 7200;

    function setUp() public {
        vm.warp(1_765_000_000);
        operator = vm.addr(operatorPk);
        vm.prank(owner);
        game = new CoordinationGame(owner, operator, treasury, SPLIT_BPS, STAKE, COMMIT_TIMEOUT, REVEAL_TIMEOUT);
        vm.deal(p1, 100 ether);
        vm.deal(p2, 100 ether);
    }

    // ----- helpers --------------------------------------------------------

    /// Force the LSB (the bit reveal reads via `uint8(r[31]) & 1`).
    function _withBit(bytes32 salt, uint8 bit) internal pure returns (bytes32) {
        return bytes32((uint256(salt) & ~uint256(1)) | uint256(bit & 1));
    }

    function _commit(uint8 guess, bytes32 salt) internal pure returns (bytes32 r, bytes32 commitment) {
        r = _withBit(salt, guess);
        commitment = sha256(abi.encodePacked(r));
    }

    function _opSig(bytes32 gameId, bytes32 commitment) internal view returns (bytes memory) {
        bytes32 digest = keccak256(abi.encode(block.chainid, address(game), gameId, commitment));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(operatorPk, digest);
        return abi.encodePacked(r, s, v);
    }

    function _create(bytes32 gameId, uint8 matchupType) internal returns (bytes32 rMatchup) {
        bytes32 mc;
        (rMatchup, mc) = _commit(matchupType, keccak256(abi.encode(gameId, "matchup")));
        vm.prank(p1);
        game.createGame{value: STAKE}(gameId, mc, _opSig(gameId, mc));
        vm.prank(p2);
        game.joinGame{value: STAKE}(gameId);
    }

    /// Run a full game to terminal resolution and assert the exact pot split.
    function _playAndCheck(
        uint8 matchupType,
        uint8 g1,
        uint8 g2,
        bool p1CommitsFirst,
        uint256 expToP1,
        uint256 expToP2,
        uint256 expGain
    ) internal {
        bytes32 gameId = keccak256(abi.encode(matchupType, g1, g2, p1CommitsFirst, "terminal"));
        bytes32 rMatchup = _create(gameId, matchupType);

        (bytes32 r1, bytes32 c1) = _commit(g1, keccak256(abi.encode(gameId, "1")));
        (bytes32 r2, bytes32 c2) = _commit(g2, keccak256(abi.encode(gameId, "2")));
        if (p1CommitsFirst) {
            vm.prank(p1);
            game.commitGuess(gameId, c1);
            vm.prank(p2);
            game.commitGuess(gameId, c2);
        } else {
            vm.prank(p2);
            game.commitGuess(gameId, c2);
            vm.prank(p1);
            game.commitGuess(gameId, c1);
        }

        uint256 p1Before = p1.balance;
        uint256 p2Before = p2.balance;
        uint256 treasBefore = treasury.balance;
        uint256 prizeBefore = game.prizePoolWei();

        vm.prank(p1);
        game.revealGuess(gameId, r1, rMatchup);
        vm.prank(p2);
        game.revealGuess(gameId, r2, bytes32(0));

        uint256 expCut = (expGain * SPLIT_BPS) / 10000;
        uint256 expPrize = expGain - expCut;
        // Pull-payment (M1): resolution credits; recipients realize it via withdraw().
        if (expToP1 > 0) {
            vm.prank(p1);
            game.withdraw();
        }
        if (expToP2 > 0) {
            vm.prank(p2);
            game.withdraw();
        }
        if (expCut > 0) {
            vm.prank(treasury);
            game.withdraw();
        }
        assertEq(p1.balance, p1Before + expToP1, "p1 payout");
        assertEq(p2.balance, p2Before + expToP2, "p2 payout");
        assertEq(treasury.balance, treasBefore + expCut, "treasury cut");
        assertEq(game.prizePoolWei(), prizeBefore + expPrize, "prize pool");
    }

    // ----- payoff matrix: terminal cells ---------------------------------

    function test_homog_bothCorrect() public {
        _playAndCheck(0, 0, 0, true, STAKE, STAKE, 0);
    }

    function test_homog_p1CorrectOnly() public {
        _playAndCheck(0, 0, 1, true, STAKE / 2, 0, 2 * STAKE - STAKE / 2);
    }

    function test_homog_p2CorrectOnly() public {
        _playAndCheck(0, 1, 0, true, 0, STAKE / 2, 2 * STAKE - STAKE / 2);
    }

    function test_homog_bothWrong() public {
        _playAndCheck(0, 1, 1, true, 0, 0, 2 * STAKE);
    }

    function test_hetero_p1WinsCorrectOnly() public {
        _playAndCheck(1, 1, 0, true, 2 * STAKE, 0, 0);
    }

    function test_hetero_p2WinsCorrectOnly() public {
        _playAndCheck(1, 0, 1, true, 0, 2 * STAKE, 0);
    }

    function test_hetero_bothCorrect_firstCommitterP1() public {
        _playAndCheck(1, 1, 1, true, 2 * STAKE, 0, 0);
    }

    function test_hetero_bothCorrect_firstCommitterP2() public {
        _playAndCheck(1, 1, 1, false, 0, 2 * STAKE, 0);
    }

    function test_hetero_bothWrong() public {
        _playAndCheck(1, 0, 0, true, 0, 0, 2 * STAKE);
    }

    /// The pot is fully accounted: after a resolved game the contract holds only
    /// the retained prize-pool share.
    function test_conservation_contractHoldsOnlyPrizePool() public {
        _playAndCheck(0, 1, 1, true, 0, 0, 2 * STAKE);
        assertEq(address(game).balance, game.prizePoolWei(), "no stranded wei");
    }

    // ----- timeout paths --------------------------------------------------

    function test_timeout_active_bothForfeit() public {
        bytes32 gameId = keccak256("to-active");
        _create(gameId, 1);
        uint256 treasBefore = treasury.balance;
        vm.warp(block.timestamp + COMMIT_TIMEOUT);
        game.resolveTimeout(gameId);
        vm.prank(treasury);
        game.withdraw();
        assertEq(treasury.balance, treasBefore + (2 * uint256(STAKE) * SPLIT_BPS) / 10000, "both forfeit");
    }

    function test_timeout_committing_committerWins() public {
        bytes32 gameId = keccak256("to-commit");
        _create(gameId, 1);
        (, bytes32 c1) = _commit(0, keccak256(abi.encode(gameId, "1")));
        vm.prank(p1);
        game.commitGuess(gameId, c1);
        uint256 p1Before = p1.balance;
        vm.warp(block.timestamp + COMMIT_TIMEOUT);
        game.resolveTimeout(gameId);
        vm.prank(p1);
        game.withdraw();
        assertEq(p1.balance, p1Before + 2 * uint256(STAKE), "committer (p1) wins pot");
    }

    function test_timeout_revealing_revealerWins() public {
        bytes32 gameId = keccak256("to-reveal");
        bytes32 rMatchup = _create(gameId, 1);
        (bytes32 r1, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        (, bytes32 c2) = _commit(1, keccak256(abi.encode(gameId, "2")));
        vm.prank(p1);
        game.commitGuess(gameId, c1);
        vm.prank(p2);
        game.commitGuess(gameId, c2);
        vm.prank(p1);
        game.revealGuess(gameId, r1, rMatchup);
        uint256 p1Before = p1.balance;
        vm.warp(block.timestamp + REVEAL_TIMEOUT);
        game.resolveTimeout(gameId);
        vm.prank(p1);
        game.withdraw();
        assertEq(p1.balance, p1Before + 2 * uint256(STAKE), "revealer (p1) wins pot");
    }

    function test_timeout_revertsBeforeDeadline() public {
        bytes32 gameId = keccak256("to-early");
        _create(gameId, 1);
        vm.expectRevert(CoordinationGame.DeadlineNotReached.selector);
        game.resolveTimeout(gameId);
    }

    // ----- rejections at the boundary ------------------------------------

    function test_revert_badOperatorSig() public {
        bytes32 gameId = keccak256("bad-sig");
        (, bytes32 mc) = _commit(0, keccak256(abi.encode(gameId, "matchup")));
        // Sign with the wrong key.
        bytes32 digest = keccak256(abi.encode(block.chainid, address(game), gameId, mc));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(0xBEEF, digest);
        vm.prank(p1);
        vm.expectRevert(CoordinationGame.BadSignature.selector);
        game.createGame{value: STAKE}(gameId, mc, abi.encodePacked(r, s, v));
    }

    function test_revert_wrongStakeOnCreate() public {
        bytes32 gameId = keccak256("bad-stake");
        (, bytes32 mc) = _commit(0, keccak256(abi.encode(gameId, "matchup")));
        vm.prank(p1);
        vm.expectRevert(CoordinationGame.BadStake.selector);
        game.createGame{value: STAKE - 1}(gameId, mc, _opSig(gameId, mc));
    }

    function test_revert_selfJoin() public {
        bytes32 gameId = keccak256("self-join");
        (, bytes32 mc) = _commit(0, keccak256(abi.encode(gameId, "matchup")));
        vm.prank(p1);
        game.createGame{value: STAKE}(gameId, mc, _opSig(gameId, mc));
        vm.prank(p1);
        vm.expectRevert(CoordinationGame.NotParticipant.selector);
        game.joinGame{value: STAKE}(gameId);
    }

    function test_revert_doubleCommit() public {
        bytes32 gameId = keccak256("double-commit");
        _create(gameId, 1);
        (, bytes32 c1) = _commit(0, keccak256(abi.encode(gameId, "1")));
        vm.prank(p1);
        game.commitGuess(gameId, c1);
        vm.prank(p1);
        vm.expectRevert(CoordinationGame.AlreadyActed.selector);
        game.commitGuess(gameId, c1);
    }

    function test_revert_revealWrongPreimage() public {
        bytes32 gameId = keccak256("bad-reveal");
        bytes32 rMatchup = _create(gameId, 1);
        (, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        (, bytes32 c2) = _commit(1, keccak256(abi.encode(gameId, "2")));
        vm.prank(p1);
        game.commitGuess(gameId, c1);
        vm.prank(p2);
        game.commitGuess(gameId, c2);
        // Reveal a preimage that does not hash to the commitment.
        vm.prank(p1);
        vm.expectRevert(CoordinationGame.CertMismatch.selector);
        game.revealGuess(gameId, keccak256("not-the-preimage"), rMatchup);
    }

    function test_revert_matchupBindingMismatch() public {
        bytes32 gameId = keccak256("bad-matchup");
        _create(gameId, 1);
        (bytes32 r1, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        (, bytes32 c2) = _commit(1, keccak256(abi.encode(gameId, "2")));
        vm.prank(p1);
        game.commitGuess(gameId, c1);
        vm.prank(p2);
        game.commitGuess(gameId, c2);
        // First revealer supplies an rMatchup that does not bind to the commitment.
        vm.prank(p1);
        vm.expectRevert(CoordinationGame.CertMismatch.selector);
        game.revealGuess(gameId, r1, keccak256("wrong-matchup"));
    }

    function test_revert_secondRevealerNonNullMatchup() public {
        bytes32 gameId = keccak256("second-matchup");
        bytes32 rMatchup = _create(gameId, 1);
        (bytes32 r1, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        (bytes32 r2, bytes32 c2) = _commit(1, keccak256(abi.encode(gameId, "2")));
        vm.prank(p1);
        game.commitGuess(gameId, c1);
        vm.prank(p2);
        game.commitGuess(gameId, c2);
        vm.prank(p1);
        game.revealGuess(gameId, r1, rMatchup);
        // Second revealer must pass the null sentinel.
        vm.prank(p2);
        vm.expectRevert(CoordinationGame.CertMismatch.selector);
        game.revealGuess(gameId, r2, rMatchup);
    }

    function test_pause_blocksCreate() public {
        bytes32 gameId = keccak256("paused");
        (, bytes32 mc) = _commit(0, keccak256(abi.encode(gameId, "matchup")));
        vm.prank(owner);
        game.pause();
        vm.prank(p1);
        vm.expectRevert();
        game.createGame{value: STAKE}(gameId, mc, _opSig(gameId, mc));
    }

    // ----- owner operations ----------------------------------------------

    function test_withdrawPrizePool() public {
        _playAndCheck(0, 1, 1, true, 0, 0, 2 * STAKE); // funds the prize pool
        uint256 prize = game.prizePoolWei();
        assertGt(prize, 0, "prize accrued");
        vm.prank(owner);
        game.withdrawPrizePool(treasury, prize);
        assertEq(game.prizePoolWei(), 0, "prize swept");
    }

    function test_revert_withdrawPrizePoolNotOwner() public {
        vm.prank(p1);
        vm.expectRevert();
        game.withdrawPrizePool(p1, 0);
    }

    function test_revert_constructorKeySeparation() public {
        // operatorSigner == treasury is rejected (key separation).
        vm.expectRevert(CoordinationGame.BadConfig.selector);
        new CoordinationGame(owner, treasury, treasury, SPLIT_BPS, STAKE, COMMIT_TIMEOUT, REVEAL_TIMEOUT);
    }

    // ----- H3 cancelPending: unjoined-game refund -------------------------

    /// A Pending game that no one joins must be refundable after the join window,
    /// else P1's stake is stranded forever.
    function test_H3_cancelPendingRefundsCreatorAfterWindow() public {
        bytes32 gameId = keccak256("h3-strand");
        (, bytes32 mc) = _commit(0, keccak256(abi.encode(gameId, "matchup")));
        vm.prank(p1);
        game.createGame{value: STAKE}(gameId, mc, _opSig(gameId, mc));

        // Too early: still inside the join window.
        vm.expectRevert(CoordinationGame.DeadlineNotReached.selector);
        game.cancelPending(gameId);

        vm.warp(block.timestamp + COMMIT_TIMEOUT + 1);
        uint256 before = p1.balance;
        // Permissionless crank (a third party calls it).
        vm.prank(address(0xCA77));
        game.cancelPending(gameId);

        assertEq(uint8(_status(gameId)), uint8(CoordinationGame.Status.Cancelled), "game cancelled");
        assertEq(game.withdrawable(p1), STAKE, "creator credited the stake");
        vm.prank(p1);
        game.withdraw();
        assertEq(p1.balance, before + STAKE, "creator made whole");
    }

    /// Once P2 joins, the game is Active and can no longer be cancelled — no
    /// griefer can refund-and-cancel a live match out from under the players.
    function test_H3_cancelPendingRevertsOnceJoined() public {
        bytes32 gameId = keccak256("h3-joined");
        _create(gameId, 1); // creates + joins → Active
        vm.warp(block.timestamp + COMMIT_TIMEOUT + 1);
        vm.expectRevert(CoordinationGame.InvalidStatus.selector);
        game.cancelPending(gameId);
    }

    // ----- M3 timeout-window snapshot -------------------------------------

    /// An owner setConfig that lowers the timeouts must NOT be able to force a
    /// running game to timeout-resolve early — the windows are snapshotted at
    /// create, so resolution still waits for the original deadline.
    function test_M3_setConfigCannotForceResolveInFlight() public {
        bytes32 gameId = keccak256("m3-inflight");
        _create(gameId, 1); // Active; windows snapshotted at COMMIT/REVEAL_TIMEOUT

        // Owner shrinks both timeouts to the floor (60) AFTER the game started.
        vm.prank(owner);
        game.setConfig(operator, treasury, SPLIT_BPS, STAKE, 60, 60);

        // Past the NEW (shrunk) commit timeout (60) but before the snapshotted one
        // (COMMIT_TIMEOUT=3600): must still be too early to resolve.
        vm.warp(block.timestamp + 100);
        vm.expectRevert(CoordinationGame.DeadlineNotReached.selector);
        game.resolveTimeout(gameId);

        // Past the snapshotted window → resolves on the original schedule.
        vm.warp(block.timestamp + COMMIT_TIMEOUT);
        game.resolveTimeout(gameId);
        assertEq(
            uint8(_status(gameId)), uint8(CoordinationGame.Status.Resolved), "resolves at the snapshotted deadline"
        );
    }

    /// L1: setConfig rejects out-of-range stake / timeout windows.
    function test_L1_setConfigRejectsOutOfRangeValues() public {
        vm.startPrank(owner);
        // stake below the dust floor
        vm.expectRevert(CoordinationGame.BadConfig.selector);
        game.setConfig(operator, treasury, SPLIT_BPS, 1, COMMIT_TIMEOUT, REVEAL_TIMEOUT);
        // commit timeout below the 60s floor
        vm.expectRevert(CoordinationGame.BadConfig.selector);
        game.setConfig(operator, treasury, SPLIT_BPS, STAKE, 59, REVEAL_TIMEOUT);
        // reveal timeout over the 30-day cap
        vm.expectRevert(CoordinationGame.BadConfig.selector);
        game.setConfig(operator, treasury, SPLIT_BPS, STAKE, COMMIT_TIMEOUT, 31 days);
        vm.stopPrank();
    }

    // ----- M1 pull-payment ------------------------------------------------

    /// A winner that reverts on receive must NOT be able to brick the terminal
    /// reveal (which used to push the pot). Under M1 the resolution credits and
    /// always succeeds; only the bad recipient's own withdraw() fails.
    function test_M1_resolveSurvivesRevertingWinner() public {
        RevertingReceiver evil = new RevertingReceiver();
        vm.deal(address(evil), 100 ether);
        bytes32 gameId = keccak256("m1-evil");

        // Hetero match (matchupType=1); evil is p2 and guesses correctly → wins.
        (bytes32 rMatchup, bytes32 mc) = _commit(1, keccak256(abi.encode(gameId, "matchup")));
        vm.prank(p1);
        game.createGame{value: STAKE}(gameId, mc, _opSig(gameId, mc));
        vm.prank(address(evil));
        game.joinGame{value: STAKE}(gameId);

        (bytes32 r1, bytes32 c1) = _commit(0, keccak256(abi.encode(gameId, "1"))); // p1 wrong
        (bytes32 r2, bytes32 c2) = _commit(1, keccak256(abi.encode(gameId, "2"))); // evil correct
        vm.prank(p1);
        game.commitGuess(gameId, c1);
        vm.prank(address(evil));
        game.commitGuess(gameId, c2);
        vm.prank(p1);
        game.revealGuess(gameId, r1, rMatchup);

        // The terminal reveal pays the pot. Under push payment evil's reverting
        // receive() would revert this call and strand the game; under M1 it
        // credits and succeeds.
        vm.prank(address(evil));
        game.revealGuess(gameId, r2, bytes32(0));

        assertEq(game.withdrawable(address(evil)), 2 * uint256(STAKE), "winner credited the full pot");
        assertEq(uint8(_status(gameId)), uint8(CoordinationGame.Status.Resolved), "game resolved despite bad winner");

        // Evil still can't pull (its receive reverts), but the protocol is intact.
        vm.prank(address(evil));
        vm.expectRevert(bytes("withdraw failed"));
        game.withdraw();
    }

    function _status(bytes32 gameId) internal view returns (CoordinationGame.Status s) {
        (s,,,,,,,,,,,,,,,,) = game.games(gameId);
    }
}

/// Reverts on any plain-ETH receive — stands in for a hostile winner/treasury.
contract RevertingReceiver {
    receive() external payable {
        revert("no ether");
    }
}
