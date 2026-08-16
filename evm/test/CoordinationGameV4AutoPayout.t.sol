// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CoordinationGameV4} from "../src/CoordinationGameV4.sol";
import {CertLib} from "../src/CertLib.sol";
import {ERC1967Proxy} from "../lib/openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";

/// Proves the WIRING of the push-preferred payout into the LIVE same-chain
/// contract: CoordinationGameV4._resolve now pays the players via {_settle}
/// (push, with the ledger as fallback) instead of always crediting. A winner
/// who has closed the page is therefore paid inside the resolving transaction —
/// matching Solana's push-at-resolve — with no later withdrawFor. The push /
/// fallback mechanics themselves are proven at the foundation in
/// PullPaymentSettleTest; this suite proves _resolve calls it.
contract CoordinationGameV4AutoPayoutTest is Test {
    CoordinationGameV4 internal game;

    uint256 internal constant operatorPk = 0xA11CE;
    address internal operator;
    address internal owner = address(0x0FFE);
    address internal treasury = address(0xCAFE);
    address internal p1 = address(0x9001);
    address internal p2 = address(0x9002);

    uint128 internal constant STAKE = 0.0027 ether;
    uint16 internal constant SPLIT_BPS = 5000;

    function setUp() public {
        vm.warp(1_765_000_000);
        operator = vm.addr(operatorPk);
        CoordinationGameV4 impl = new CoordinationGameV4();
        bytes memory init = abi.encodeCall(
            CoordinationGameV4.initialize, (owner, operator, treasury, SPLIT_BPS, STAKE, 3600, 7200, 1, 365 days)
        );
        game = CoordinationGameV4(payable(address(new ERC1967Proxy(address(impl), init))));
        vm.deal(p1, 10 ether);
        vm.deal(p2, 10 ether);
    }

    // ----- helpers --------------------------------------------------------

    function _withBit(bytes32 salt, uint8 bit) internal pure returns (bytes32) {
        return bytes32((uint256(salt) & ~uint256(1)) | uint256(bit & 1));
    }

    function _commit(uint8 guess, bytes32 salt) internal pure returns (bytes32 r, bytes32 commitment) {
        r = _withBit(salt, guess);
        commitment = sha256(abi.encodePacked(r));
    }

    function _opSig(bytes32 gameId, bytes32 commitment, address creator) internal view returns (bytes memory) {
        bytes32 digest = keccak256(abi.encode(block.chainid, address(game), gameId, creator, commitment));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(operatorPk, digest);
        return abi.encodePacked(r, s, v);
    }

    function _joinSig(bytes32 gameId, address joiner) internal view returns (bytes memory) {
        bytes32 digest = keccak256(abi.encode(block.chainid, address(game), gameId, joiner));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(operatorPk, digest);
        return abi.encodePacked(r, s, v);
    }

    /// createGame (p1) + joinGame (p2), both staking inline. Returns rMatchup.
    function _createAndJoin(bytes32 gameId, uint8 matchupType) internal returns (bytes32 rMatchup) {
        bytes32 mc;
        (rMatchup, mc) = _commit(matchupType, keccak256(abi.encode(gameId, "matchup")));
        vm.prank(p1);
        game.createGame{value: STAKE}(gameId, mc, _opSig(gameId, mc, p1), p1);
        vm.prank(p2);
        game.joinGame{value: STAKE}(gameId, p2, _joinSig(gameId, p2));
    }

    /// Drive a game up to (but NOT including) the resolving reveal, so the caller
    /// can measure balance deltas across resolution alone. p1 commits+reveals
    /// first; the returned `r2` is p2's reveal preimage that triggers resolution.
    function _playToRevealReady(bytes32 gameId, uint8 matchupType, uint8 g1, uint8 g2) internal returns (bytes32 r2) {
        bytes32 rMatchup = _createAndJoin(gameId, matchupType);
        (bytes32 r1, bytes32 c1) = _commit(g1, keccak256(abi.encode(gameId, "1")));
        bytes32 c2;
        (r2, c2) = _commit(g2, keccak256(abi.encode(gameId, "2")));
        vm.prank(p1);
        game.commitGuess(gameId, c1);
        vm.prank(p2);
        game.commitGuess(gameId, c2);
        vm.prank(p1);
        game.revealGuess(gameId, r1, rMatchup);
    }

    // ----- the property this change exists for ----------------------------

    /// HETERO_P1_WINS (matchup=1, p1 correct, p2 wrong) pays p1 the whole pot.
    /// Measured across the resolving reveal alone: p1's BALANCE rises by the full
    /// pot with no withdrawFor, and nothing is parked in the ledger. This is the
    /// exact scenario that stranded on mainnet before the fix.
    function test_winnerIsPaidAtResolve_noWithdrawNeeded() public {
        bytes32 gameId = keccak256("auto-p1-wins");
        bytes32 r2 = _playToRevealReady(gameId, 1, 1, 0);

        uint256 p1Before = p1.balance;
        vm.prank(p2);
        game.revealGuess(gameId, r2, bytes32(0));

        assertEq(p1.balance, p1Before + 2 * STAKE, "winner paid the whole pot at resolve, no withdraw");
        assertEq(game.withdrawable(p1), 0, "nothing stranded in the pull ledger");
    }

    /// HOMOG_BOTH_CORRECT returns each player their own stake — BOTH are pushed
    /// back at resolve, and neither has to pull.
    function test_bothCorrectPushesBothStakesBack() public {
        bytes32 gameId = keccak256("auto-both-correct");
        bytes32 r2 = _playToRevealReady(gameId, 0, 0, 0);

        uint256 p1Before = p1.balance;
        uint256 p2Before = p2.balance;
        vm.prank(p2);
        game.revealGuess(gameId, r2, bytes32(0));

        assertEq(p1.balance, p1Before + STAKE, "p1 stake pushed back");
        assertEq(p2.balance, p2Before + STAKE, "p2 stake pushed back");
        assertEq(game.withdrawable(p1), 0);
        assertEq(game.withdrawable(p2), 0);
    }

    /// The treasury share is NOT pushed per game — it accrues via the ledger and
    /// is swept by the DAO. HOMOG_P1_CORRECT: p1 gets half its stake back
    /// (pushed), the forfeit splits treasury (credited) + season prize pool.
    function test_treasuryShareStaysCredited_notPushed() public {
        bytes32 gameId = keccak256("auto-treasury");
        bytes32 r2 = _playToRevealReady(gameId, 0, 0, 1); // HOMOG_P1_CORRECT: toP1 = STAKE/2

        uint256 p1Before = p1.balance;
        uint256 treasuryBalBefore = treasury.balance;
        vm.prank(p2);
        game.revealGuess(gameId, r2, bytes32(0));

        assertEq(p1.balance, p1Before + STAKE / 2, "p1's half-stake pushed at resolve");
        assertEq(treasury.balance, treasuryBalBefore, "treasury NOT paid inline");
        assertGt(game.withdrawable(treasury), 0, "treasury share credited to the ledger for the DAO to sweep");
    }

    /// A timeout resolution (a player abandons after committing) also pushes the
    /// winner's payout — the resolveTimeout crank pays through the same _resolve.
    function test_timeoutResolutionAlsoPushesWinner() public {
        bytes32 gameId = keccak256("auto-timeout");
        bytes32 rMatchup = _createAndJoin(gameId, 1);
        (bytes32 r1, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        (, bytes32 c2) = _commit(0, keccak256(abi.encode(gameId, "2")));
        vm.prank(p1);
        game.commitGuess(gameId, c1);
        vm.prank(p2);
        game.commitGuess(gameId, c2);
        // Only p1 reveals; p2 abandons. p1 is the present, honest revealer.
        vm.prank(p1);
        game.revealGuess(gameId, r1, rMatchup);

        uint256 p1Before = p1.balance;
        // Reveal window elapses; anyone cranks the timeout.
        vm.warp(block.timestamp + 7200 + 1);
        game.resolveTimeout(gameId);

        assertGt(p1.balance, p1Before, "the present player is paid at the timeout crank, no withdraw");
        assertEq(game.withdrawable(p1), 0, "nothing stranded");
    }
}
