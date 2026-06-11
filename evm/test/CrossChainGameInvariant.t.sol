// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CrossChainGame} from "../src/CrossChainGame.sol";
import {CertLib} from "../src/CertLib.sol";

/// @dev Handler driving randomized lifecycles. Every action keeps a
///      running tally of stake ETH that has entered the contract and is
///      not yet refunded/settled, so the invariant can check the contract
///      never holds less than it owes.
contract Handler is Test {
    CrossChainGame public game;

    uint256 public operatorPk = 0xA11CE;
    uint256 public localPk = 0xB0B;
    uint256 public counterPk = 0xCA7;
    address public player = makeAddr("invPlayer");

    uint128 public constant STAKE = 0.0025 ether;
    uint128 public constant MAX_TRANCHE = 0.005 ether;
    uint32 public constant CLAIM_WINDOW = 3600;

    bytes32[] public openMatches;
    uint256 public createdCount;

    constructor(CrossChainGame game_) {
        game = game_;
        vm.deal(player, 1000 ether);
    }

    function _legB(bytes32 id, uint128 tranche) internal view returns (CertLib.Leg memory) {
        return CertLib.Leg({
            chainTag: keccak256("eip155:84532"),
            contractId: bytes32(uint256(uint160(address(game)))),
            player: bytes32(uint256(uint160(player))),
            sessionKey: vm.addr(localPk),
            stake: STAKE,
            tranche: tranche
        });
    }

    function _cert(bytes32 id, uint128 tranche) internal view returns (CertLib.MatchLiveCert memory) {
        return CertLib.MatchLiveCert({
            matchId: id,
            tournamentId: 1,
            matchupCommitment: bytes32(0),
            legA: CertLib.Leg({
                chainTag: keccak256("solana:devnet"),
                contractId: bytes32(uint256(1)),
                player: bytes32(uint256(2)),
                sessionKey: vm.addr(counterPk),
                stake: 0.05 ether,
                tranche: tranche
            }),
            legB: _legB(id, tranche),
            quoteTimestamp: uint64(block.timestamp),
            quoteMaxAgeSecs: 600,
            matchDeadline: uint64(block.timestamp + 2 days),
            claimWindowSecs: CLAIM_WINDOW,
            aIsP1: 1
        });
    }

    function _sign(uint256 pk, bytes32 d) internal view returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, d);
        return abi.encodePacked(r, s, v);
    }

    function createAndLock(uint128 trancheSeed) external {
        uint128 tranche = uint128(bound(trancheSeed, 0, MAX_TRANCHE));
        if (game.poolFree() < tranche) return;
        bytes32 id = keccak256(abi.encode("inv", createdCount));
        createdCount++;
        vm.prank(player);
        game.createMatch{value: STAKE}(
            id,
            vm.addr(localPk),
            vm.addr(counterPk),
            true,
            uint64(block.timestamp + 1 days),
            uint64(block.timestamp + 2 days)
        );
        game.lockTranche(id, tranche);
        openMatches.push(id);
    }

    function settleOne(uint256 idx, uint8 rawKind) external {
        if (openMatches.length == 0) return;
        idx = bound(idx, 0, openMatches.length - 1);
        bytes32 id = openMatches[idx];
        // Match fields: status(0) … stakeWei(7), trancheWei(8) …
        (CrossChainGame.Status status,,,,,,,, uint128 tranche,,,,,,,) = game.matches(id);
        if (status != CrossChainGame.Status.Locked) return;

        uint8 kind = rawKind % 9;
        CertLib.MatchLiveCert memory cert = _cert(id, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.OutcomeCert memory oc = CertLib.OutcomeCert({
            matchId: id,
            matchLiveDigest: liveDigest,
            outcomeKind: kind,
            stepCount: CertLib.TERMINAL_STEP_COUNT,
            p1Guess: 1,
            p2Guess: 0,
            firstCommitter: 1,
            matchupType: 1,
            transcriptHash: bytes32(0)
        });
        bytes[3] memory liveSigs =
            [_sign(counterPk, liveDigest), _sign(localPk, liveDigest), _sign(operatorPk, liveDigest)];
        bytes32 ocDigest = CertLib.outcomeDigest(oc);
        bytes[3] memory ocSigs = [_sign(counterPk, ocDigest), _sign(localPk, ocDigest), _sign(operatorPk, ocDigest)];
        game.settle(cert, oc, liveSigs, ocSigs);
    }

    function fundPool(uint96 amount) external {
        uint256 amt = bound(amount, 0, 5 ether);
        vm.prank(player);
        game.poolDeposit{value: amt}();
    }
}

/// @notice Invariant: the contract's ETH balance always equals the sum of
///         its tracked buckets (poolFree + poolLocked + prizePool + live
///         stake escrows). No wei is ever created, destroyed, or stranded
///         outside the accounting.
contract CrossChainGameInvariantTest is Test {
    CrossChainGame internal game;
    Handler internal handler;

    address internal owner = makeAddr("invOwner");
    address internal treasury = makeAddr("invTreasury");

    function setUp() public {
        vm.warp(1_765_000_000);
        vm.prank(owner);
        game = new CrossChainGame(
            keccak256("eip155:84532"), owner, vm.addr(0xA11CE), treasury, 5000, 0.0025 ether, 0.005 ether, 3600, 900
        );
        vm.deal(owner, 100 ether);
        vm.prank(owner);
        game.poolDeposit{value: 50 ether}();

        handler = new Handler(game);
        // The handler must own the game to call lockTranche; transfer
        // ownership via the 2-step flow.
        vm.prank(owner);
        game.transferOwnership(address(handler));
        vm.prank(address(handler));
        game.acceptOwnership();

        targetContract(address(handler));
    }

    /// poolFree + poolLocked + prizePool + (stake per live match) must
    /// equal the contract's actual ETH balance, always.
    function invariant_trackedBalancesReconcile() public view {
        uint256 liveStakes;
        uint256 n = handler.createdCount();
        for (uint256 i = 0; i < n; i++) {
            bytes32 id = keccak256(abi.encode("inv", i));
            // Match fields: status(0), player(1) … playerIsP1(4),
            // localEquivocated(5), counterEquivocated(6), stakeWei(7) …
            (CrossChainGame.Status status,,,,,,, uint128 stakeWei,,,,,,,,) = game.matches(id);
            if (
                status == CrossChainGame.Status.Funded || status == CrossChainGame.Status.Locked
                    || status == CrossChainGame.Status.Claiming
            ) {
                liveStakes += stakeWei;
            }
        }
        uint256 tracked = game.poolFree() + game.poolLocked() + game.prizePoolWei() + liveStakes;
        assertEq(address(game).balance, tracked, "contract balance must reconcile with accounting");
    }

    /// Locked tranches can never exceed what the pool actually holds.
    function invariant_poolNeverOverLocked() public view {
        assertLe(game.poolLocked(), address(game).balance, "locked exceeds balance");
    }
}
