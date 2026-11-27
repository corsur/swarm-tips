// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CrossChainGame} from "../src/CrossChainGame.sol";
import {CertLib} from "../src/CertLib.sol";
import {ERC1967Proxy} from "../lib/openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";

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
    address public treasury;

    uint128 public constant STAKE = 0.0025 ether;
    uint128 public constant MAX_TRANCHE = 0.005 ether;
    uint32 public constant CLAIM_WINDOW = 3600;
    bytes32 public constant R_MATCHUP = bytes32(uint256(0xE1));

    struct MatchMeta {
        uint64 quoteTimestamp;
        uint64 matchDeadline;
        uint128 tranche;
    }

    bytes32[] public openMatches;
    mapping(bytes32 => MatchMeta) public meta;
    uint256 public createdCount;

    constructor(CrossChainGame game_, address treasury_) {
        game = game_;
        treasury = treasury_;
        vm.deal(player, 1000 ether);
    }

    function _legB(uint128 tranche) internal view returns (CertLib.Leg memory) {
        return CertLib.Leg({
            chainTag: keccak256("eip155:84532"),
            contractId: bytes32(uint256(uint160(address(game)))),
            player: bytes32(uint256(uint160(player))),
            sessionKey: vm.addr(localPk),
            stake: STAKE,
            tranche: tranche
        });
    }

    function _cert(bytes32 id) internal view returns (CertLib.MatchLiveCert memory) {
        MatchMeta memory mm = meta[id];
        return CertLib.MatchLiveCert({
            matchId: id,
            tournamentId: 1,
            matchupCommitment: sha256(abi.encodePacked(R_MATCHUP)),
            legA: CertLib.Leg({
                chainTag: keccak256("solana:devnet"),
                contractId: bytes32(uint256(1)),
                player: bytes32(uint256(2)),
                sessionKey: vm.addr(counterPk),
                stake: 0.05 ether,
                tranche: mm.tranche
            }),
            legB: _legB(mm.tranche),
            quoteTimestamp: mm.quoteTimestamp,
            quoteMaxAgeSecs: 600,
            matchDeadline: mm.matchDeadline,
            claimWindowSecs: CLAIM_WINDOW,
            // Seat invariant: createMatch uses playerIsP1=true, so the EVM leg
            // (player) is P1 → aIsP1=0 (the contract enforces (aIsP1==1) != playerIsP1).
            aIsP1: 0
        });
    }

    function _sign(uint256 pk, bytes32 d) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, d);
        return abi.encodePacked(r, s, v);
    }

    function _checkpoint(bytes32 liveDigest, uint8 stepCount) internal pure returns (CertLib.Checkpoint memory) {
        uint8 p1Guess = stepCount >= 3 ? 1 : 255;
        uint8 p2Guess = stepCount == 4 ? 0 : 255;
        return CertLib.Checkpoint({
            matchLiveDigest: liveDigest,
            stepCount: stepCount,
            p1Commit: keccak256("p1c"),
            p2Commit: keccak256("p2c"),
            p1Guess: p1Guess,
            p2Guess: p2Guess,
            firstCommitter: 1,
            matchupType: 1,
            transcriptHash: keccak256(abi.encode("t", stepCount)),
            rMatchup: R_MATCHUP
        });
    }

    function _createFunded() internal returns (bytes32 id) {
        id = keccak256(abi.encode("inv", createdCount));
        createdCount++;
        uint64 fundDeadline = uint64(block.timestamp + 1 days);
        uint64 matchDeadline = uint64(block.timestamp + 2 days);
        bytes32 createDigest = keccak256(
            abi.encode(
                block.chainid,
                address(game),
                id,
                player,
                vm.addr(localPk),
                vm.addr(counterPk),
                true,
                fundDeadline,
                matchDeadline
            )
        );
        vm.prank(player);
        game.createMatch{value: STAKE}(
            id, vm.addr(localPk), vm.addr(counterPk), true, fundDeadline, matchDeadline, _sign(operatorPk, createDigest)
        );
        meta[id] = MatchMeta({quoteTimestamp: uint64(block.timestamp), matchDeadline: matchDeadline, tranche: 0});
        openMatches.push(id);
    }

    function createOnly() external {
        _createFunded();
    }

    function createAndLock(uint128 trancheSeed) external {
        uint128 tranche = uint128(bound(trancheSeed, 0, MAX_TRANCHE));
        if (game.poolFree() < tranche) return;
        // Respect M6 rolling-24h cap so handler calls don't revert.
        bool newWindow = block.timestamp >= uint256(game.trancheWindowStart()) + 1 days;
        uint256 windowLocked = newWindow ? 0 : game.trancheLockedInWindow();
        if (windowLocked + tranche > game.dailyTrancheCapWei()) return;

        bytes32 id = _createFunded();
        meta[id].tranche = tranche;
        CertLib.MatchLiveCert memory cert = _cert(id);
        game.lockTranche(cert, _sign(operatorPk, CertLib.matchLiveDigest(cert)));
    }

    function settleOne(uint256 idx, uint8 rawKind) external {
        if (openMatches.length == 0) return;
        idx = bound(idx, 0, openMatches.length - 1);
        bytes32 id = openMatches[idx];
        (CrossChainGame.Status status,,,,, bool localEq, bool counterEq,,,,,,,,,,) = game.matches(id);
        bool canSettle = status == CrossChainGame.Status.Locked
            || (status == CrossChainGame.Status.Claiming && !localEq && !counterEq);
        if (!canSettle) return;

        uint8 kind = rawKind % 9;
        CertLib.MatchLiveCert memory cert = _cert(id);
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

    function openClaimOne(uint256 idx, uint8 stepSeed) external {
        if (openMatches.length == 0) return;
        idx = bound(idx, 0, openMatches.length - 1);
        bytes32 id = openMatches[idx];
        (CrossChainGame.Status status,,,,, bool localEq,,,,,,, uint64 windowEnd,,,,) = game.matches(id);
        if (status != CrossChainGame.Status.Locked || localEq || block.timestamp > windowEnd) return;

        uint8 step = stepSeed % 5; // 0..=4
        CertLib.MatchLiveCert memory cert = _cert(id);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.Checkpoint memory cp = _checkpoint(liveDigest, step);
        bytes32 cpDigest = CertLib.checkpointDigest(cp);
        bytes[3] memory liveSigs =
            [_sign(counterPk, liveDigest), _sign(localPk, liveDigest), _sign(operatorPk, liveDigest)];
        bytes[2] memory cpSigs = [_sign(counterPk, cpDigest), _sign(localPk, cpDigest)];
        game.openClaim(cert, cp, liveSigs, cpSigs);
    }

    function supersedeClaimOne(uint256 idx, uint8 stepSeed) external {
        if (openMatches.length == 0) return;
        idx = bound(idx, 0, openMatches.length - 1);
        bytes32 id = openMatches[idx];
        (CrossChainGame.Status status,,,,, bool localEq,,,,,,, uint64 windowEnd,, uint8 bestStep,,) = game.matches(id);
        if (
            status != CrossChainGame.Status.Claiming || localEq || block.timestamp > windowEnd
                || bestStep >= CertLib.TERMINAL_STEP_COUNT
        ) return;

        uint8 nextStep = uint8(bound(stepSeed, bestStep + 1, CertLib.TERMINAL_STEP_COUNT));
        CertLib.MatchLiveCert memory cert = _cert(id);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.Checkpoint memory cp = _checkpoint(liveDigest, nextStep);
        bytes32 cpDigest = CertLib.checkpointDigest(cp);
        bytes[2] memory cpSigs = [_sign(counterPk, cpDigest), _sign(localPk, cpDigest)];
        game.supersedeClaim(cert, cp, cpSigs);
    }

    function settleClaimOne(uint256 idx) external {
        if (openMatches.length == 0) return;
        idx = bound(idx, 0, openMatches.length - 1);
        bytes32 id = openMatches[idx];
        (CrossChainGame.Status status,,,,, bool localEq,,,,,,, uint64 windowEnd,,,,) = game.matches(id);
        if (status != CrossChainGame.Status.Claiming) return;
        if (!localEq && block.timestamp <= windowEnd) return;
        game.settleClaim(id);
    }

    function refundNoCertOne(uint256 idx) external {
        if (openMatches.length == 0) return;
        idx = bound(idx, 0, openMatches.length - 1);
        bytes32 id = openMatches[idx];
        (CrossChainGame.Status status,,,,,,,,, uint64 fundDeadline,,,,,,,) = game.matches(id);
        if (status != CrossChainGame.Status.Funded || block.timestamp <= fundDeadline) return;
        game.refundNoCert(id);
    }

    function refundTimeoutOne(uint256 idx) external {
        if (openMatches.length == 0) return;
        idx = bound(idx, 0, openMatches.length - 1);
        bytes32 id = openMatches[idx];
        (CrossChainGame.Status status,,,,, bool localEq,,,,,,,,,,, uint64 timeoutOpensAt) = game.matches(id);
        if (status != CrossChainGame.Status.Locked && status != CrossChainGame.Status.Claiming) return;
        if (localEq || block.timestamp <= timeoutOpensAt) return;
        game.refundTimeout(id);
    }

    function withdrawOne(bool pullTreasury) external {
        address target = pullTreasury ? treasury : player;
        if (game.withdrawable(target) == 0) return;
        vm.prank(target);
        game.withdraw();
    }

    function warp(uint256 deltaSeed) external {
        uint256 delta = bound(deltaSeed, 1 minutes, 3 days);
        vm.warp(block.timestamp + delta);
    }

    function fundPool(uint96 amount) external {
        uint256 amt = bound(amount, 0, 5 ether);
        vm.prank(player);
        game.poolDeposit{value: amt}();
    }
}

/// @notice Invariant: the contract's ETH balance always equals the sum of
///         its tracked buckets (poolFree + poolLocked + prizePool + live
///         stake escrows + withdrawable credits). No wei is ever created,
///         destroyed, or stranded outside the accounting across all paths.
contract CrossChainGameInvariantTest is Test {
    CrossChainGame internal game;
    Handler internal handler;

    address internal owner = makeAddr("invOwner");
    address internal treasury = makeAddr("invTreasury");

    function setUp() public {
        vm.warp(1_765_000_000);
        CrossChainGame impl = new CrossChainGame();
        bytes memory init = abi.encodeCall(
            CrossChainGame.initialize,
            (
                keccak256("eip155:84532"),
                owner,
                vm.addr(0xA11CE),
                treasury,
                5000,
                0.0025 ether,
                0.005 ether,
                100 ether, // M6 daily tranche cap: above the 50-ether pool, so never binds here
                3600,
                900
            )
        );
        game = CrossChainGame(payable(address(new ERC1967Proxy(address(impl), init))));
        vm.deal(owner, 100 ether);
        vm.prank(owner);
        game.poolDeposit{value: 50 ether}();

        handler = new Handler(game, treasury);
        // lockTranche is permissionless now (operator-sig authorized), so the
        // handler no longer needs to own the game.
        targetContract(address(handler));
    }

    /// poolFree + poolLocked + prizePool + (stake per live match) + credits must
    /// equal the contract's actual ETH balance, always.
    function invariant_trackedBalancesReconcile() public view {
        uint256 liveStakes;
        uint256 n = handler.createdCount();
        for (uint256 i = 0; i < n; i++) {
            bytes32 id = keccak256(abi.encode("inv", i));
            // Match fields: status(0), player(1) … playerIsP1(4),
            // localEquivocated(5), counterEquivocated(6), stakeWei(7) …
            (CrossChainGame.Status status,,,,,,, uint128 stakeWei,,,,,,,,,) = game.matches(id);
            if (
                status == CrossChainGame.Status.Funded || status == CrossChainGame.Status.Locked
                    || status == CrossChainGame.Status.Claiming
            ) {
                liveStakes += stakeWei;
            }
        }
        uint256 credited = game.withdrawable(handler.player()) + game.withdrawable(treasury);
        uint256 tracked = game.poolFree() + game.poolLocked() + game.prizePoolWei() + liveStakes + credited;
        assertEq(address(game).balance, tracked, "contract balance must reconcile with accounting");
    }
}
