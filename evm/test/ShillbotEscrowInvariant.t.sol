// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {ShillbotEscrow} from "../src/ShillbotEscrow.sol";
import {VerifyLib} from "../src/VerifyLib.sol";

/// @dev Handler driving randomized task lifecycles (the CrossChainGame
///      invariant-harness style): every action guards the contract's own
///      preconditions and no-ops when they don't hold, so the runner explores
///      deep interleavings without reverting.
contract Handler is Test {
    ShillbotEscrow public esc;

    uint256 public constant attesterPk = 0xA77E57;
    address public client = makeAddr("invClient");
    address public worker = makeAddr("invWorker");
    address public challenger = makeAddr("invChallenger");
    address public owner;

    uint64 public createdCount;

    constructor(ShillbotEscrow esc_, address owner_) {
        esc = esc_;
        owner = owner_;
        vm.deal(client, 1e30);
        vm.deal(worker, 1e30);
        vm.deal(challenger, 1e30);
    }

    function _sign(bytes32 digest) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(attesterPk, digest);
        return abi.encodePacked(r, s, v);
    }

    function _pick(uint256 idxSeed) internal view returns (uint64 id, ShillbotEscrow.Task memory t, bool ok) {
        if (createdCount == 0) return (0, t, false);
        id = uint64(bound(idxSeed, 0, createdCount - 1));
        t = esc.getTask(id);
        ok = true;
    }

    function createTask(uint96 escrowSeed, bool kind1, bool requiresApproval) external {
        uint256 escrow = bound(escrowSeed, esc.minEscrowWei(), 2 ether);
        vm.prank(client);
        esc.createTask{value: escrow}(
            keccak256("inv-statement"),
            keccak256("inv-policy"),
            kind1 ? 1 : 0,
            uint64(block.timestamp + 2 days),
            requiresApproval
        );
        createdCount++;
    }

    function claim(uint256 idxSeed) external {
        (uint64 id, ShillbotEscrow.Task memory t, bool ok) = _pick(idxSeed);
        if (!ok || t.state != ShillbotEscrow.TaskState.Open || block.timestamp >= t.deadline) return;
        vm.prank(worker);
        esc.claimTask(id);
    }

    function submit(uint256 idxSeed) external {
        (uint64 id, ShillbotEscrow.Task memory t, bool ok) = _pick(idxSeed);
        if (!ok || t.state != ShillbotEscrow.TaskState.Claimed || block.timestamp >= t.deadline) return;
        vm.prank(worker);
        esc.submitWork(id, keccak256(abi.encode("content", id)), keccak256(abi.encode("artifact", id)));
    }

    function approve(uint256 idxSeed) external {
        (uint64 id, ShillbotEscrow.Task memory t, bool ok) = _pick(idxSeed);
        if (!ok || t.state != ShillbotEscrow.TaskState.Submitted || !t.requiresApproval) return;
        vm.prank(client);
        esc.approveTask(id);
    }

    function verify(uint256 idxSeed, uint64 scoreSeed) external {
        (uint64 id, ShillbotEscrow.Task memory t, bool ok) = _pick(idxSeed);
        if (!ok) return;
        ShillbotEscrow.TaskState required =
            t.requiresApproval ? ShillbotEscrow.TaskState.Approved : ShillbotEscrow.TaskState.Submitted;
        if (t.state != required) return;
        uint64 score;
        if (t.verificationKind == VerifyLib.KIND_DETERMINISTIC_ATTESTED) {
            score = scoreSeed % 2 == 0 ? uint64(VerifyLib.MAX_SCORE) : 0;
        } else {
            score = uint64(bound(scoreSeed, 0, VerifyLib.MAX_SCORE));
        }
        bytes32 digest = VerifyLib.attestationDigest(
            esc.CHAIN_TAG(),
            bytes32(uint256(uint160(address(esc)))),
            id,
            VerifyLib.Descriptor(t.verificationKind, t.statementCommitment, t.policyId, t.artifactHash),
            score
        );
        esc.verifyTaskAttested(id, score, _sign(digest));
    }

    function challenge(uint256 idxSeed) external {
        (uint64 id, ShillbotEscrow.Task memory t, bool ok) = _pick(idxSeed);
        if (!ok || t.state != ShillbotEscrow.TaskState.Verified || block.timestamp >= t.challengeDeadline) return;
        uint256 bond = uint256(t.escrowWei) * esc.challengeBondMultiplier();
        vm.prank(challenger);
        esc.challengeTask{value: bond}(id);
    }

    function finalize(uint256 idxSeed) external {
        (uint64 id, ShillbotEscrow.Task memory t, bool ok) = _pick(idxSeed);
        if (!ok || t.state != ShillbotEscrow.TaskState.Verified || block.timestamp <= t.challengeDeadline) return;
        esc.finalizeTask(id);
    }

    function resolve(uint256 idxSeed, bool challengerWon) external {
        (uint64 id, ShillbotEscrow.Task memory t, bool ok) = _pick(idxSeed);
        if (!ok || t.state != ShillbotEscrow.TaskState.Disputed || block.timestamp > t.resolutionDeadline) return;
        vm.prank(owner);
        esc.resolveChallenge(id, challengerWon);
    }

    function defaultResolve(uint256 idxSeed) external {
        (uint64 id, ShillbotEscrow.Task memory t, bool ok) = _pick(idxSeed);
        if (!ok || t.state != ShillbotEscrow.TaskState.Disputed || block.timestamp <= t.resolutionDeadline) return;
        esc.defaultResolve(id);
    }

    function expire(uint256 idxSeed) external {
        (uint64 id, ShillbotEscrow.Task memory t, bool ok) = _pick(idxSeed);
        if (!ok) return;
        if (t.state == ShillbotEscrow.TaskState.Open || t.state == ShillbotEscrow.TaskState.Claimed) {
            if (block.timestamp <= t.deadline) return;
        } else if (t.state == ShillbotEscrow.TaskState.Submitted || t.state == ShillbotEscrow.TaskState.Approved) {
            if (block.timestamp <= uint256(t.submittedAt) + t.verificationTimeoutSecs) return;
        } else {
            return;
        }
        esc.expireTask(id);
    }

    function withdrawOne(uint256 actorSeed) external {
        address[4] memory actors = [client, worker, challenger, esc.treasury()];
        address actor = actors[bound(actorSeed, 0, 3)];
        if (esc.withdrawable(actor) == 0) return;
        vm.prank(actor);
        esc.withdraw();
    }

    function warp(uint256 secsSeed) external {
        vm.warp(block.timestamp + bound(secsSeed, 1, 12 hours));
    }
}

/// @notice Invariant: the escrow's ETH balance always equals the sum of its
///         tracked buckets — live task escrows + disputed bonds + accrued
///         withdrawable credits. No wei is ever created, destroyed, or
///         stranded outside the accounting.
contract ShillbotEscrowInvariantTest is Test {
    ShillbotEscrow internal esc;
    Handler internal handler;

    address internal owner = makeAddr("invOwner");
    address internal treasury = makeAddr("invTreasury");

    function setUp() public {
        vm.warp(1_765_000_000);
        esc = new ShillbotEscrow(
            keccak256("eip155:84532"),
            owner,
            ShillbotEscrow.Config({
                attesterSigner: vm.addr(0xA77E57),
                treasury: treasury,
                protocolFeeBps: 1000,
                qualityThreshold: 200_000,
                challengeWindowSecs: 3600,
                disputeWindowSecs: 1 days,
                verificationTimeoutSecs: 2 days,
                challengeBondMultiplier: 2,
                bondSlashTreasuryBps: 5000,
                minEscrowWei: 0.001 ether,
                maxEscrowWei: 100 ether
            })
        );
        handler = new Handler(esc, owner);
        targetContract(address(handler));
    }

    function _holdsEscrow(ShillbotEscrow.TaskState s) internal pure returns (bool) {
        return s == ShillbotEscrow.TaskState.Open || s == ShillbotEscrow.TaskState.Claimed
            || s == ShillbotEscrow.TaskState.Submitted || s == ShillbotEscrow.TaskState.Approved
            || s == ShillbotEscrow.TaskState.Verified || s == ShillbotEscrow.TaskState.Disputed;
    }

    function invariant_balanceReconciles() public view {
        uint256 liveEscrow;
        uint256 liveBonds;
        uint64 n = handler.createdCount();
        for (uint64 i = 0; i < n; i++) {
            ShillbotEscrow.Task memory t = esc.getTask(i);
            if (_holdsEscrow(t.state)) liveEscrow += t.escrowWei;
            if (t.state == ShillbotEscrow.TaskState.Disputed) liveBonds += t.bondWei;
        }
        uint256 credited = esc.withdrawable(handler.client()) + esc.withdrawable(handler.worker())
            + esc.withdrawable(handler.challenger()) + esc.withdrawable(treasury);
        assertEq(address(esc).balance, liveEscrow + liveBonds + credited, "balance must reconcile with the accounting");
    }

    function invariant_taskCounterMatchesHandler() public view {
        assertEq(esc.nextTaskId(), handler.createdCount(), "every created task is counted exactly once");
    }
}

/// @notice Fuzz: the kind-0 continuum payment must match an independent
///         re-implementation of the Solana `scoring::compute_payment`
///         formula for arbitrary (score, threshold, escrow, feeBps) inside
///         the config bounds — pinning integer-division semantics.
contract ShillbotEscrowPaymentFuzz is Test {
    ShillbotEscrow internal esc;

    uint256 internal constant attesterPk = 0xA77E57;
    address internal owner = makeAddr("fuzzOwner");
    address internal treasury = makeAddr("fuzzTreasury");
    address internal client = makeAddr("fuzzClient");
    address internal worker = makeAddr("fuzzWorker");

    function setUp() public {
        vm.warp(1_765_000_000);
        esc = new ShillbotEscrow(keccak256("eip155:84532"), owner, _cfg(200_000, 1000));
        vm.deal(client, 1000 ether);
    }

    function _cfg(uint64 threshold, uint16 feeBps) internal view returns (ShillbotEscrow.Config memory) {
        return ShillbotEscrow.Config({
            attesterSigner: vm.addr(attesterPk),
            treasury: treasury,
            protocolFeeBps: feeBps,
            qualityThreshold: threshold,
            challengeWindowSecs: 3600,
            disputeWindowSecs: 1 days,
            verificationTimeoutSecs: 2 days,
            challengeBondMultiplier: 2,
            bondSlashTreasuryBps: 5000,
            minEscrowWei: 0.001 ether,
            maxEscrowWei: 100 ether
        });
    }

    /// Independent mirror of scoring.rs::compute_payment (u128 intermediates
    /// there, uint256 here — both exact for these ranges).
    function _reference(uint64 score, uint64 threshold, uint128 escrow, uint16 feeBps)
        internal
        pure
        returns (uint256 payment, uint256 fee)
    {
        if (score < threshold) return (0, 0);
        uint256 range = 1_000_000 - uint256(threshold);
        uint256 gross = (uint256(escrow) * (uint256(score) - threshold)) / range;
        fee = (gross * feeBps) / 10_000;
        payment = gross - fee;
    }

    function testFuzz_kind0Payment_matchesReference(uint64 score, uint64 threshold, uint128 escrow, uint16 feeBps)
        public
    {
        score = uint64(bound(score, 0, 1_000_000));
        threshold = uint64(bound(threshold, 0, 999_999));
        escrow = uint128(bound(escrow, 0.001 ether, 100 ether));
        feeBps = uint16(bound(feeBps, 100, 2500));

        vm.prank(owner);
        esc.setConfig(_cfg(threshold, feeBps));

        vm.prank(client);
        uint64 id =
            esc.createTask{value: escrow}(keccak256("s"), keccak256("p"), 0, uint64(block.timestamp + 1 days), false);
        vm.prank(worker);
        esc.claimTask(id);
        vm.prank(worker);
        esc.submitWork(id, keccak256("c"), keccak256("a"));

        ShillbotEscrow.Task memory t = esc.getTask(id);
        bytes32 digest = VerifyLib.attestationDigest(
            esc.CHAIN_TAG(),
            bytes32(uint256(uint160(address(esc)))),
            id,
            VerifyLib.Descriptor(t.verificationKind, t.statementCommitment, t.policyId, t.artifactHash),
            score
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(attesterPk, digest);
        esc.verifyTaskAttested(id, score, abi.encodePacked(r, s, v));

        (uint256 refPayment, uint256 refFee) = _reference(score, threshold, escrow, feeBps);
        t = esc.getTask(id);
        assertEq(t.paymentWei, refPayment, "payment drift vs reference formula");
        assertEq(t.feeWei, refFee, "fee drift vs reference formula");
        assertLe(uint256(t.paymentWei) + t.feeWei, escrow, "conservation over the escrow");
    }
}
