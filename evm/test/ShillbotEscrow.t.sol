// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {ShillbotEscrow} from "../src/ShillbotEscrow.sol";
import {VerifyLib} from "../src/VerifyLib.sol";
import {PullPayment} from "../src/base/PullPayment.sol";
import {ChainTagged} from "../src/base/ChainTagged.sol";
import {BoundaryBattery} from "./helpers/BoundaryBattery.sol";

/// @notice Behavioral suite for the Shillbot EVM escrow: full lifecycle per
///         verification kind, every revert path, the attester-signature
///         tamper battery, deadline boundary batteries (via the shared
///         BoundaryBattery helper), pause policy, and Ownable2Step handover.
///         Payment expectations mirror `programs/shillbot/src/scoring.rs`.
contract ShillbotEscrowTest is BoundaryBattery {
    ShillbotEscrow internal esc;

    uint256 internal constant attesterPk = 0xA77E57;
    address internal attester;
    address internal owner = makeAddr("owner");
    address internal treasury = makeAddr("treasury");
    address internal client = makeAddr("client");
    address internal worker = makeAddr("worker");
    address internal challenger = makeAddr("challenger");

    bytes32 internal constant CHAIN_TAG = keccak256("eip155:84532");
    bytes32 internal constant STATEMENT = keccak256("statement");
    bytes32 internal constant POLICY = keccak256("policy");
    bytes32 internal constant CONTENT = keccak256("content");
    bytes32 internal constant ARTIFACT = keccak256("artifact");

    uint128 internal constant ESCROW = 1 ether;
    uint16 internal constant FEE_BPS = 1000; // 10%
    uint64 internal constant THRESHOLD = 200_000;
    uint32 internal constant CHALLENGE_WINDOW = 3600;
    uint32 internal constant DISPUTE_WINDOW = 86_400;
    uint32 internal constant VERIFICATION_TIMEOUT = 172_800;
    uint8 internal constant BOND_MULTIPLIER = 2;
    uint16 internal constant BOND_SLASH_BPS = 5000; // 50/50 worker/treasury
    uint256 internal constant BOND = uint256(ESCROW) * BOND_MULTIPLIER;

    /// Task id under test for the BoundaryBattery callbacks (internal
    /// function pointers can't close over locals).
    uint64 internal batteryId;

    function setUp() public {
        vm.warp(1_765_000_000);
        attester = vm.addr(attesterPk);
        esc = new ShillbotEscrow(CHAIN_TAG, owner, _cfg());
        vm.deal(client, 1000 ether);
        vm.deal(worker, 1000 ether);
        vm.deal(challenger, 1000 ether);
    }

    // ----- helpers --------------------------------------------------------

    function _cfg() internal view returns (ShillbotEscrow.Config memory) {
        return ShillbotEscrow.Config({
            attesterSigner: attester,
            treasury: treasury,
            protocolFeeBps: FEE_BPS,
            qualityThreshold: THRESHOLD,
            challengeWindowSecs: CHALLENGE_WINDOW,
            disputeWindowSecs: DISPUTE_WINDOW,
            verificationTimeoutSecs: VERIFICATION_TIMEOUT,
            challengeBondMultiplier: BOND_MULTIPLIER,
            bondSlashTreasuryBps: BOND_SLASH_BPS,
            minEscrowWei: 0.001 ether,
            maxEscrowWei: 100 ether
        });
    }

    function _create(uint8 kind, bool requiresApproval) internal returns (uint64 id) {
        vm.prank(client);
        id = esc.createTask{value: ESCROW}(STATEMENT, POLICY, kind, uint64(block.timestamp + 1 days), requiresApproval);
    }

    function _claim(uint64 id) internal {
        vm.prank(worker);
        esc.claimTask(id);
    }

    function _submit(uint64 id) internal {
        vm.prank(worker);
        esc.submitWork(id, CONTENT, ARTIFACT);
    }

    /// The canonical attestation digest for a task ON a given escrow, from
    /// its stored descriptor fields — mirrors what the attester service signs.
    function _digestOn(ShillbotEscrow target, uint64 id, uint64 score) internal view returns (bytes32) {
        ShillbotEscrow.Task memory t = target.getTask(id);
        return VerifyLib.attestationDigest(
            target.CHAIN_TAG(),
            bytes32(uint256(uint160(address(target)))),
            id,
            VerifyLib.Descriptor(t.verificationKind, t.statementCommitment, t.policyId, t.artifactHash),
            score
        );
    }

    function _sign(uint256 pk, bytes32 digest) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, digest);
        return abi.encodePacked(r, s, v);
    }

    function _verify(uint64 id, uint64 score) internal {
        esc.verifyTaskAttested(id, score, _sign(attesterPk, _digestOn(esc, id, score)));
    }

    /// expectRevert binds to the NEXT external call, so the sig (whose digest
    /// helper calls getTask) must be built before arming the revert.
    function _verifyExpectRevert(uint64 id, uint64 score, bytes4 err) internal {
        bytes memory sig = _sign(attesterPk, _digestOn(esc, id, score));
        vm.expectRevert(err);
        esc.verifyTaskAttested(id, score, sig);
    }

    /// Full path to Verified with a kind-1 passing score.
    function _toVerified() internal returns (uint64 id) {
        id = _create(1, false);
        _claim(id);
        _submit(id);
        _verify(id, VerifyLib.MAX_SCORE);
    }

    /// Full path to Disputed.
    function _toDisputed() internal returns (uint64 id) {
        id = _toVerified();
        vm.prank(challenger);
        esc.challengeTask{value: BOND}(id);
    }

    function _state(uint64 id) internal view returns (ShillbotEscrow.TaskState) {
        return esc.getTask(id).state;
    }

    // ----- constructor + config -------------------------------------------

    function test_chainTag_isKeccakOfCaip2String() public view {
        assertEq(esc.CHAIN_TAG(), keccak256("eip155:84532"), "CHAIN_TAG must be the CAIP-2 hash");
    }

    function test_constructor_storesConfigAndOwner() public view {
        assertEq(esc.owner(), owner);
        assertEq(esc.attesterSigner(), attester);
        assertEq(esc.treasury(), treasury);
        assertEq(esc.protocolFeeBps(), FEE_BPS);
        assertEq(esc.qualityThreshold(), THRESHOLD);
        assertEq(esc.challengeWindowSecs(), CHALLENGE_WINDOW);
        assertEq(esc.disputeWindowSecs(), DISPUTE_WINDOW);
        assertEq(esc.verificationTimeoutSecs(), VERIFICATION_TIMEOUT);
        assertEq(esc.challengeBondMultiplier(), BOND_MULTIPLIER);
        assertEq(esc.bondSlashTreasuryBps(), BOND_SLASH_BPS);
        assertEq(esc.minEscrowWei(), 0.001 ether);
        assertEq(esc.maxEscrowWei(), 100 ether);
        assertEq(esc.nextTaskId(), 0);
    }

    function test_stateEnumValues_matchSolanaWireValues() public pure {
        assertEq(uint8(ShillbotEscrow.TaskState.Open), 0);
        assertEq(uint8(ShillbotEscrow.TaskState.Claimed), 1);
        assertEq(uint8(ShillbotEscrow.TaskState.Submitted), 2);
        assertEq(uint8(ShillbotEscrow.TaskState.Verified), 3);
        assertEq(uint8(ShillbotEscrow.TaskState.Finalized), 4);
        assertEq(uint8(ShillbotEscrow.TaskState.Disputed), 5);
        assertEq(uint8(ShillbotEscrow.TaskState.Resolved), 6);
        assertEq(uint8(ShillbotEscrow.TaskState.Approved), 7);
        assertEq(uint8(ShillbotEscrow.TaskState.DefaultResolved), 8);
    }

    function test_revert_zeroChainTag() public {
        vm.expectRevert(ChainTagged.BadChainTag.selector);
        new ShillbotEscrow(bytes32(0), owner, _cfg());
    }

    function _expectBadConfig(ShillbotEscrow.Config memory cfg) internal {
        vm.expectRevert(ShillbotEscrow.BadConfig.selector);
        new ShillbotEscrow(CHAIN_TAG, owner, cfg);
    }

    function test_revert_configBounds() public {
        ShillbotEscrow.Config memory cfg;

        cfg = _cfg();
        cfg.attesterSigner = address(0);
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.treasury = address(0);
        _expectBadConfig(cfg);
        // Key separation: attester is never the owner or the treasury.
        cfg = _cfg();
        cfg.attesterSigner = owner;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.attesterSigner = treasury;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.protocolFeeBps = 99;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.protocolFeeBps = 2501;
        _expectBadConfig(cfg);
        // Fail loud: threshold == MAX_SCORE silently pays 0.
        cfg = _cfg();
        cfg.qualityThreshold = uint64(VerifyLib.MAX_SCORE);
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.challengeWindowSecs = 59;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.challengeWindowSecs = 30 days + 1;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.disputeWindowSecs = 1 hours - 1;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.disputeWindowSecs = 30 days + 1;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.verificationTimeoutSecs = 1 hours - 1;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.verificationTimeoutSecs = 90 days + 1;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.challengeBondMultiplier = 1;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.challengeBondMultiplier = 11;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.bondSlashTreasuryBps = 10_001;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.minEscrowWei = 0;
        _expectBadConfig(cfg);
        cfg = _cfg();
        cfg.maxEscrowWei = cfg.minEscrowWei - 1;
        _expectBadConfig(cfg);
    }

    function test_setConfig_ownerOnlyAndApplied() public {
        ShillbotEscrow.Config memory cfg = _cfg();
        cfg.protocolFeeBps = 500;

        vm.prank(client);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, client));
        esc.setConfig(cfg);

        vm.prank(owner);
        esc.setConfig(cfg);
        assertEq(esc.protocolFeeBps(), 500, "setConfig applied");
    }

    // ----- createTask -------------------------------------------------------

    function test_createTask_storesFieldsAndSnapshotsWindows() public {
        uint64 deadline = uint64(block.timestamp + 1 days);
        vm.prank(client);
        uint64 id = esc.createTask{value: ESCROW}(STATEMENT, POLICY, 1, deadline, true);

        assertEq(id, 0);
        assertEq(esc.nextTaskId(), 1, "counter bumped");
        ShillbotEscrow.Task memory t = esc.getTask(id);
        assertEq(t.client, client);
        assertEq(uint8(t.state), uint8(ShillbotEscrow.TaskState.Open));
        assertEq(t.verificationKind, 1);
        assertTrue(t.requiresApproval);
        assertEq(t.escrowWei, ESCROW);
        assertEq(t.statementCommitment, STATEMENT);
        assertEq(t.policyId, POLICY);
        assertEq(t.deadline, deadline);
        assertEq(t.challengeWindowSecs, CHALLENGE_WINDOW, "challenge window snapshotted");
        assertEq(t.disputeWindowSecs, DISPUTE_WINDOW, "dispute window snapshotted");
        assertEq(t.verificationTimeoutSecs, VERIFICATION_TIMEOUT, "verification timeout snapshotted");
        assertEq(address(esc).balance, ESCROW, "escrow held");
    }

    function test_createTask_kind0AllowsZeroCommitments() public {
        vm.prank(client);
        uint64 id = esc.createTask{value: ESCROW}(bytes32(0), bytes32(0), 0, uint64(block.timestamp + 1 days), false);
        assertEq(uint8(_state(id)), uint8(ShillbotEscrow.TaskState.Open));
    }

    function test_revert_createTask_bounds() public {
        uint64 deadline = uint64(block.timestamp + 1 days);
        vm.startPrank(client);
        // Escrow outside [min, max].
        vm.expectRevert(ShillbotEscrow.BadEscrow.selector);
        esc.createTask{value: 0.001 ether - 1}(STATEMENT, POLICY, 1, deadline, false);
        vm.expectRevert(ShillbotEscrow.BadEscrow.selector);
        esc.createTask{value: 100 ether + 1}(STATEMENT, POLICY, 1, deadline, false);
        // Deadline must be strictly in the future.
        vm.expectRevert(ShillbotEscrow.DeadlinePassed.selector);
        esc.createTask{value: ESCROW}(STATEMENT, POLICY, 1, uint64(block.timestamp), false);
        // Unknown verification kind.
        vm.expectRevert(ShillbotEscrow.BadKind.selector);
        esc.createTask{value: ESCROW}(STATEMENT, POLICY, 2, deadline, false);
        // Kind 1 requires the statement + policy commitments.
        vm.expectRevert(ShillbotEscrow.BadCommitment.selector);
        esc.createTask{value: ESCROW}(bytes32(0), POLICY, 1, deadline, false);
        vm.expectRevert(ShillbotEscrow.BadCommitment.selector);
        esc.createTask{value: ESCROW}(STATEMENT, bytes32(0), 1, deadline, false);
        vm.stopPrank();
    }

    // ----- claimTask --------------------------------------------------------

    function test_claimTask_setsWorker() public {
        uint64 id = _create(1, false);
        _claim(id);
        ShillbotEscrow.Task memory t = esc.getTask(id);
        assertEq(t.worker, worker);
        assertEq(uint8(t.state), uint8(ShillbotEscrow.TaskState.Claimed));
    }

    function test_revert_claimTask_rejections() public {
        uint64 id = _create(0, false);
        // Nonexistent id.
        vm.prank(worker);
        vm.expectRevert(ShillbotEscrow.InvalidStatus.selector);
        esc.claimTask(id + 1);
        // Arms-length for BOTH kinds: the client can't claim its own task.
        vm.prank(client);
        vm.expectRevert(ShillbotEscrow.NotParticipant.selector);
        esc.claimTask(id);
        // Double claim.
        _claim(id);
        vm.prank(challenger);
        vm.expectRevert(ShillbotEscrow.InvalidStatus.selector);
        esc.claimTask(id);
    }

    function _actClaim() internal {
        vm.prank(worker);
        esc.claimTask(batteryId);
    }

    /// Claim closes strictly BEFORE the deadline; the deadline second itself
    /// is dead (expiry opens strictly after — no overlap, no gap ambiguity).
    function test_boundary_claimTask_deadline() public {
        batteryId = _create(1, false);
        assertLiveStrictlyBefore(esc.getTask(batteryId).deadline, ShillbotEscrow.DeadlinePassed.selector, _actClaim);
    }

    // ----- submitWork -------------------------------------------------------

    function test_submitWork_stampsSubmission() public {
        uint64 id = _create(1, false);
        _claim(id);
        _submit(id);
        ShillbotEscrow.Task memory t = esc.getTask(id);
        assertEq(t.contentIdHash, CONTENT);
        assertEq(t.artifactHash, ARTIFACT);
        assertEq(t.submittedAt, uint64(block.timestamp));
        assertEq(uint8(t.state), uint8(ShillbotEscrow.TaskState.Submitted));
    }

    function test_revert_submitWork_rejections() public {
        uint64 id = _create(1, false);
        // Not yet claimed.
        vm.prank(worker);
        vm.expectRevert(ShillbotEscrow.InvalidStatus.selector);
        esc.submitWork(id, CONTENT, ARTIFACT);
        _claim(id);
        // Only the claiming worker may submit.
        vm.prank(challenger);
        vm.expectRevert(ShillbotEscrow.NotParticipant.selector);
        esc.submitWork(id, CONTENT, ARTIFACT);
        // Zero hashes can never verify — rejected at the boundary.
        vm.startPrank(worker);
        vm.expectRevert(ShillbotEscrow.BadCommitment.selector);
        esc.submitWork(id, bytes32(0), ARTIFACT);
        vm.expectRevert(ShillbotEscrow.BadCommitment.selector);
        esc.submitWork(id, CONTENT, bytes32(0));
        // Past the deadline (dead second at == too, mirroring claim).
        vm.warp(esc.getTask(id).deadline);
        vm.expectRevert(ShillbotEscrow.DeadlinePassed.selector);
        esc.submitWork(id, CONTENT, ARTIFACT);
        vm.stopPrank();
    }

    // ----- approveTask ------------------------------------------------------

    function test_approveTask_gatesVerification() public {
        uint64 id = _create(1, true);
        _claim(id);
        _submit(id);
        // Verification requires Approved for requiresApproval tasks.
        _verifyExpectRevert(id, VerifyLib.MAX_SCORE, ShillbotEscrow.InvalidStatus.selector);

        vm.prank(client);
        esc.approveTask(id);
        assertEq(uint8(_state(id)), uint8(ShillbotEscrow.TaskState.Approved));

        _verify(id, VerifyLib.MAX_SCORE);
        assertEq(uint8(_state(id)), uint8(ShillbotEscrow.TaskState.Verified));
    }

    function test_revert_approveTask_rejections() public {
        uint64 approvalTask = _create(1, true);
        _claim(approvalTask);
        // Not Submitted yet.
        vm.prank(client);
        vm.expectRevert(ShillbotEscrow.InvalidStatus.selector);
        esc.approveTask(approvalTask);
        _submit(approvalTask);
        // Only the client approves.
        vm.prank(worker);
        vm.expectRevert(ShillbotEscrow.NotParticipant.selector);
        esc.approveTask(approvalTask);
        // Tasks without requiresApproval can't be approved.
        uint64 plain = _create(1, false);
        _claim(plain);
        _submit(plain);
        vm.prank(client);
        vm.expectRevert(ShillbotEscrow.InvalidStatus.selector);
        esc.approveTask(plain);
    }

    // ----- verifyTaskAttested ----------------------------------------------

    function test_verify_kind1Pass_pinsFullEscrowMinusFee() public {
        uint64 id = _create(1, false);
        _claim(id);
        _submit(id);
        uint64 verifyTime = uint64(block.timestamp);
        // Permissionless relay: a random address submits the attester's sig.
        vm.prank(makeAddr("relayer"));
        esc.verifyTaskAttested(id, VerifyLib.MAX_SCORE, _sign(attesterPk, _digestOn(esc, id, VerifyLib.MAX_SCORE)));

        ShillbotEscrow.Task memory t = esc.getTask(id);
        uint256 expectedFee = (uint256(ESCROW) * FEE_BPS) / 10_000;
        assertEq(t.feeWei, expectedFee, "fee = escrow * feeBps");
        assertEq(t.paymentWei, ESCROW - expectedFee, "payment = escrow - fee");
        assertEq(t.verifiedAt, verifyTime);
        assertEq(t.challengeDeadline, verifyTime + CHALLENGE_WINDOW, "challenge window from snapshot");
        assertEq(uint8(t.state), uint8(ShillbotEscrow.TaskState.Verified));
    }

    function test_verify_kind1Fail_pinsZero() public {
        uint64 id = _create(1, false);
        _claim(id);
        _submit(id);
        _verify(id, 0);
        ShillbotEscrow.Task memory t = esc.getTask(id);
        assertEq(t.paymentWei, 0, "failing proof pays nothing");
        assertEq(t.feeWei, 0, "no fee on a failing proof");
        assertEq(uint8(t.state), uint8(ShillbotEscrow.TaskState.Verified));
    }

    function test_revert_verify_kind1NonBinaryScore() public {
        uint64 id = _create(1, false);
        _claim(id);
        _submit(id);
        _verifyExpectRevert(id, 500_000, ShillbotEscrow.BadScore.selector);
    }

    function test_verify_kind0Midpoint_matchesSolanaFormula() public {
        // Mirrors scoring.rs `payment_midpoint_score`: threshold 200k,
        // score 600k → gross = escrow * 400k/800k, fee 10%.
        uint64 id = _create(0, false);
        _claim(id);
        _submit(id);
        _verify(id, 600_000);
        ShillbotEscrow.Task memory t = esc.getTask(id);
        assertEq(t.paymentWei, 0.45 ether, "payment = gross - fee");
        assertEq(t.feeWei, 0.05 ether, "fee = gross * 10%");
    }

    function test_verify_kind0BelowThreshold_pinsZero() public {
        uint64 id = _create(0, false);
        _claim(id);
        _submit(id);
        _verify(id, THRESHOLD - 1);
        ShillbotEscrow.Task memory t = esc.getTask(id);
        assertEq(t.paymentWei, 0);
        assertEq(t.feeWei, 0);
    }

    function test_revert_verify_kind0ScoreAboveMax() public {
        uint64 id = _create(0, false);
        _claim(id);
        _submit(id);
        _verifyExpectRevert(id, uint64(VerifyLib.MAX_SCORE + 1), ShillbotEscrow.BadScore.selector);
    }

    function test_revert_verify_wrongStateAndDoubleVerify() public {
        uint64 id = _create(1, false);
        _claim(id);
        // Claimed, not Submitted.
        _verifyExpectRevert(id, VerifyLib.MAX_SCORE, ShillbotEscrow.InvalidStatus.selector);
        _submit(id);
        _verify(id, VerifyLib.MAX_SCORE);
        // Already Verified.
        _verifyExpectRevert(id, VerifyLib.MAX_SCORE, ShillbotEscrow.InvalidStatus.selector);
    }

    function test_revert_verify_armsLength_attesterIsWorker() public {
        uint64 id = _create(1, false);
        vm.prank(attester);
        esc.claimTask(id);
        vm.prank(attester);
        esc.submitWork(id, CONTENT, ARTIFACT);
        _verifyExpectRevert(id, VerifyLib.MAX_SCORE, ShillbotEscrow.NotParticipant.selector);
    }

    // ----- signature tamper battery ----------------------------------------

    function test_revert_verify_wrongKey() public {
        uint64 id = _create(1, false);
        _claim(id);
        _submit(id);
        bytes memory wrongKeySig = _sign(0xBEEF, _digestOn(esc, id, VerifyLib.MAX_SCORE));
        vm.expectRevert(ShillbotEscrow.BadSignature.selector);
        esc.verifyTaskAttested(id, VerifyLib.MAX_SCORE, wrongKeySig);
    }

    function test_revert_verify_tamperedScore() public {
        uint64 id = _create(1, false);
        _claim(id);
        _submit(id);
        // Signed for MAX (pass), relayed with 0 (fail): 0 passes the binary
        // score gate but the digest binds the score → BadSignature.
        bytes memory sigForMax = _sign(attesterPk, _digestOn(esc, id, VerifyLib.MAX_SCORE));
        vm.expectRevert(ShillbotEscrow.BadSignature.selector);
        esc.verifyTaskAttested(id, 0, sigForMax);
    }

    function test_revert_verify_replayOntoSecondTask() public {
        // Two byte-identical tasks; the digest binds the task id, so the
        // attestation for one can't pay out the other.
        uint64 a = _create(1, false);
        uint64 b = _create(1, false);
        _claim(a);
        _claim(b);
        _submit(a);
        _submit(b);
        bytes memory sigForA = _sign(attesterPk, _digestOn(esc, a, VerifyLib.MAX_SCORE));
        vm.expectRevert(ShillbotEscrow.BadSignature.selector);
        esc.verifyTaskAttested(b, VerifyLib.MAX_SCORE, sigForA);
        // The bound task still verifies.
        esc.verifyTaskAttested(a, VerifyLib.MAX_SCORE, sigForA);
    }

    function test_revert_verify_tamperedArtifactHash() public {
        uint64 id = _create(1, false);
        _claim(id);
        _submit(id);
        // Attester signed a digest over a DIFFERENT artifact than the one
        // submitted on-chain.
        ShillbotEscrow.Task memory t = esc.getTask(id);
        bytes32 digest = VerifyLib.attestationDigest(
            CHAIN_TAG,
            bytes32(uint256(uint160(address(esc)))),
            id,
            VerifyLib.Descriptor(t.verificationKind, t.statementCommitment, t.policyId, keccak256("other-artifact")),
            VerifyLib.MAX_SCORE
        );
        vm.expectRevert(ShillbotEscrow.BadSignature.selector);
        esc.verifyTaskAttested(id, VerifyLib.MAX_SCORE, _sign(attesterPk, digest));
    }

    function test_revert_verify_chainTagAndContractBinding() public {
        // Same lifecycle on two more escrows: one on another chain tag, one
        // on the same tag (different address). An attestation authored for
        // THIS escrow must verify on neither.
        ShillbotEscrow otherChain = new ShillbotEscrow(keccak256("eip155:1"), owner, _cfg());
        ShillbotEscrow sameChain = new ShillbotEscrow(CHAIN_TAG, owner, _cfg());

        uint64 id = _create(1, false);
        _claim(id);
        _submit(id);
        for (uint256 i = 0; i < 2; i++) {
            ShillbotEscrow target = i == 0 ? otherChain : sameChain;
            vm.prank(client);
            uint64 tid = target.createTask{value: ESCROW}(STATEMENT, POLICY, 1, uint64(block.timestamp + 1 days), false);
            vm.prank(worker);
            target.claimTask(tid);
            vm.prank(worker);
            target.submitWork(tid, CONTENT, ARTIFACT);
            // Signature over the ORIGINAL escrow's digest (its chain tag +
            // contract id) — must not execute on the other deployment.
            bytes memory foreignSig = _sign(attesterPk, _digestOn(esc, id, VerifyLib.MAX_SCORE));
            vm.expectRevert(ShillbotEscrow.BadSignature.selector);
            target.verifyTaskAttested(tid, VerifyLib.MAX_SCORE, foreignSig);
        }
    }

    // ----- challengeTask ----------------------------------------------------

    function test_challengeTask_armsResolutionDeadline() public {
        uint64 id = _toVerified();
        vm.prank(challenger);
        esc.challengeTask{value: BOND}(id);

        ShillbotEscrow.Task memory t = esc.getTask(id);
        assertEq(uint8(t.state), uint8(ShillbotEscrow.TaskState.Disputed));
        assertEq(t.challenger, challenger);
        assertEq(t.bondWei, BOND);
        assertEq(t.resolutionDeadline, uint64(block.timestamp) + DISPUTE_WINDOW, "resolution deadline ALWAYS armed");
        assertEq(address(esc).balance, ESCROW + BOND, "escrow + bond held");
    }

    function test_revert_challengeTask_bondMustBeExact() public {
        uint64 id = _toVerified();
        vm.startPrank(challenger);
        vm.expectRevert(ShillbotEscrow.BadBond.selector);
        esc.challengeTask{value: BOND - 1}(id);
        vm.expectRevert(ShillbotEscrow.BadBond.selector);
        esc.challengeTask{value: BOND + 1}(id);
        vm.stopPrank();
    }

    function test_revert_challengeTask_wrongState() public {
        uint64 id = _create(1, false);
        vm.prank(challenger);
        vm.expectRevert(ShillbotEscrow.InvalidStatus.selector);
        esc.challengeTask{value: BOND}(id);
    }

    function _actChallenge() internal {
        vm.prank(challenger);
        esc.challengeTask{value: BOND}(batteryId);
    }

    function _actFinalize() internal {
        esc.finalizeTask(batteryId);
    }

    /// challengeDeadline battery: challenge is `t <` (dead second at ==),
    /// finalize is `t >` — together the boundary second belongs to NEITHER,
    /// so a challenge and a finalize can never both be valid in one block.
    function test_boundary_challengeDeadline() public {
        batteryId = _toVerified();
        uint256 deadline = esc.getTask(batteryId).challengeDeadline;
        assertLiveStrictlyBefore(deadline, ShillbotEscrow.DeadlinePassed.selector, _actChallenge);
        assertLiveStrictlyAfter(deadline, ShillbotEscrow.DeadlineNotReached.selector, _actFinalize);
    }

    // ----- finalizeTask -----------------------------------------------------

    function test_finalizeTask_paysPinnedAmounts() public {
        // kind-0 midpoint so the remainder leg is nonzero.
        uint64 id = _create(0, false);
        _claim(id);
        _submit(id);
        _verify(id, 600_000);
        vm.warp(esc.getTask(id).challengeDeadline + 1);
        esc.finalizeTask(id);

        assertEq(uint8(_state(id)), uint8(ShillbotEscrow.TaskState.Finalized));
        assertEq(esc.withdrawable(worker), 0.45 ether, "worker credited pinned payment");
        assertEq(esc.withdrawable(treasury), 0.05 ether, "treasury credited pinned fee");
        assertEq(esc.withdrawable(client), 0.5 ether, "client credited the remainder");

        uint256 before = worker.balance;
        vm.prank(worker);
        esc.withdraw();
        assertEq(worker.balance, before + 0.45 ether, "withdraw realizes the credit");
        assertEq(esc.withdrawable(worker), 0);
        vm.prank(worker);
        vm.expectRevert(PullPayment.NothingToWithdraw.selector);
        esc.withdraw();
    }

    function test_finalizeTask_failingProofRefundsClient() public {
        uint64 id = _create(1, false);
        _claim(id);
        _submit(id);
        _verify(id, 0);
        vm.warp(esc.getTask(id).challengeDeadline + 1);
        esc.finalizeTask(id);
        assertEq(esc.withdrawable(worker), 0, "failing proof pays nothing");
        assertEq(esc.withdrawable(client), ESCROW, "full escrow home to the client");
    }

    function test_revert_finalizeTask_wrongState() public {
        uint64 id = _toDisputed();
        vm.warp(block.timestamp + 30 days);
        vm.expectRevert(ShillbotEscrow.InvalidStatus.selector);
        esc.finalizeTask(id);
    }

    // ----- resolveChallenge -------------------------------------------------

    function test_resolveChallenge_challengerWon() public {
        uint64 id = _toDisputed();
        vm.prank(owner);
        esc.resolveChallenge(id, true);

        assertEq(uint8(_state(id)), uint8(ShillbotEscrow.TaskState.Resolved));
        assertEq(esc.withdrawable(client), ESCROW, "escrow home to the client");
        assertEq(esc.withdrawable(challenger), BOND, "bond back to the challenger");
        assertEq(esc.withdrawable(worker), 0, "worker gets nothing");
        assertEq(esc.withdrawable(treasury), 0, "no fee on a rejected task");
    }

    function test_resolveChallenge_workerWon_slashesBond() public {
        uint64 id = _toDisputed();
        ShillbotEscrow.Task memory t = esc.getTask(id);
        vm.prank(owner);
        esc.resolveChallenge(id, false);

        uint256 bondHalf = BOND / 2; // bondSlashTreasuryBps = 5000
        assertEq(esc.withdrawable(worker), uint256(t.paymentWei) + (BOND - bondHalf), "payment + bond share");
        assertEq(esc.withdrawable(treasury), uint256(t.feeWei) + bondHalf, "fee + bond share");
        assertEq(esc.withdrawable(client), ESCROW - t.paymentWei - t.feeWei, "remainder");
        assertEq(esc.withdrawable(challenger), 0, "challenger loses the bond");
        // Conservation: everything credited, nothing stranded.
        uint256 credited = esc.withdrawable(worker) + esc.withdrawable(treasury) + esc.withdrawable(client);
        assertEq(credited, ESCROW + BOND, "escrow + bond fully routed");
    }

    function test_revert_resolveChallenge_notOwner() public {
        uint64 id = _toDisputed();
        vm.prank(challenger);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, challenger));
        esc.resolveChallenge(id, true);
    }

    function _actResolveChallenge() internal {
        vm.prank(owner);
        esc.resolveChallenge(batteryId, false);
    }

    function _actDefaultResolve() internal {
        esc.defaultResolve(batteryId);
    }

    /// resolutionDeadline battery: owner adjudication runs THROUGH the
    /// deadline second (`<=`), the permissionless default strictly after
    /// (`>`) — a clean handoff with no overlap and no dead second.
    function test_boundary_resolutionDeadline() public {
        batteryId = _toDisputed();
        uint256 deadline = esc.getTask(batteryId).resolutionDeadline;
        assertLiveThrough(deadline, ShillbotEscrow.DeadlinePassed.selector, _actResolveChallenge);
        assertLiveStrictlyAfter(deadline, ShillbotEscrow.DeadlineNotReached.selector, _actDefaultResolve);
    }

    // ----- defaultResolve ---------------------------------------------------

    function test_defaultResolve_paysPinnedAndReturnsBondUnslashed() public {
        uint64 id = _toDisputed();
        ShillbotEscrow.Task memory t = esc.getTask(id);
        vm.warp(uint256(t.resolutionDeadline) + 1);
        // Permissionless liveness crank.
        vm.prank(makeAddr("crank"));
        esc.defaultResolve(id);

        assertEq(uint8(_state(id)), uint8(ShillbotEscrow.TaskState.DefaultResolved));
        assertEq(esc.withdrawable(worker), t.paymentWei, "pinned payment executes");
        assertEq(esc.withdrawable(treasury), t.feeWei, "pinned fee executes");
        assertEq(esc.withdrawable(client), ESCROW - t.paymentWei - t.feeWei, "remainder");
        assertEq(esc.withdrawable(challenger), BOND, "bond back UN-SLASHED");
    }

    function test_defaultResolve_worksWhilePaused() public {
        uint64 id = _toDisputed();
        vm.warp(uint256(esc.getTask(id).resolutionDeadline) + 1);
        vm.prank(owner);
        esc.pause();
        esc.defaultResolve(id); // liveness path must survive a pause
        assertEq(uint8(_state(id)), uint8(ShillbotEscrow.TaskState.DefaultResolved));
    }

    // ----- expireTask -------------------------------------------------------

    function test_expireTask_openAndClaimed() public {
        uint64 openTask = _create(1, false);
        uint64 claimedTask = _create(1, false);
        _claim(claimedTask);
        vm.warp(uint256(esc.getTask(openTask).deadline) + 1);

        esc.expireTask(openTask);
        esc.expireTask(claimedTask);
        assertEq(uint8(_state(openTask)), uint8(ShillbotEscrow.TaskState.Resolved), "EVM expiry maps to Resolved");
        assertEq(uint8(_state(claimedTask)), uint8(ShillbotEscrow.TaskState.Resolved));
        assertEq(esc.withdrawable(client), 2 * uint256(ESCROW), "both escrows home");
    }

    function _actExpire() internal {
        esc.expireTask(batteryId);
    }

    /// Verification-timeout battery: Submitted tasks expire strictly after
    /// submittedAt + the SNAPSHOTTED timeout.
    function test_boundary_expireTask_verificationTimeout() public {
        batteryId = _create(1, false);
        _claim(batteryId);
        _submit(batteryId);
        ShillbotEscrow.Task memory t = esc.getTask(batteryId);
        assertLiveStrictlyAfter(
            uint256(t.submittedAt) + t.verificationTimeoutSecs, ShillbotEscrow.DeadlineNotReached.selector, _actExpire
        );
    }

    function test_expireTask_approvedUsesSubmittedAtAnchor() public {
        // Approval does NOT reset the verification-timeout clock.
        uint64 id = _create(1, true);
        _claim(id);
        _submit(id);
        uint64 submittedAt = esc.getTask(id).submittedAt;
        vm.warp(block.timestamp + 1000);
        vm.prank(client);
        esc.approveTask(id);
        vm.warp(uint256(submittedAt) + VERIFICATION_TIMEOUT + 1);
        esc.expireTask(id);
        assertEq(uint8(_state(id)), uint8(ShillbotEscrow.TaskState.Resolved));
        assertEq(esc.withdrawable(client), ESCROW);
    }

    function test_revert_expireTask_wrongState() public {
        uint64 id = _toVerified();
        vm.warp(block.timestamp + 365 days);
        vm.expectRevert(ShillbotEscrow.InvalidStatus.selector);
        esc.expireTask(id);
    }

    function test_expireTask_worksWhilePaused() public {
        uint64 id = _create(1, false);
        vm.warp(uint256(esc.getTask(id).deadline) + 1);
        vm.prank(owner);
        esc.pause();
        esc.expireTask(id); // liveness path must survive a pause
        assertEq(esc.withdrawable(client), ESCROW);
    }

    // ----- pause policy -----------------------------------------------------

    function test_pause_blocksStateChangingEntryPoints() public {
        uint64 verified = _toVerified();
        uint64 disputed = _toDisputed();
        uint64 claimed = _create(1, false);
        _claim(claimed);
        uint64 open = _create(1, false);

        vm.prank(owner);
        esc.pause();

        vm.prank(client);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        esc.createTask{value: ESCROW}(STATEMENT, POLICY, 1, uint64(block.timestamp + 1 days), false);
        vm.prank(worker);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        esc.claimTask(open);
        vm.prank(worker);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        esc.submitWork(claimed, CONTENT, ARTIFACT);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        esc.verifyTaskAttested(claimed, 0, "");
        vm.prank(client);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        esc.approveTask(claimed);
        vm.prank(challenger);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        esc.challengeTask{value: BOND}(verified);
        vm.prank(owner);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        esc.resolveChallenge(disputed, true);
        vm.warp(uint256(esc.getTask(verified).challengeDeadline) + 1);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        esc.finalizeTask(verified);

        // After unpause, the frozen finalize goes through.
        vm.prank(owner);
        esc.unpause();
        esc.finalizeTask(verified);
    }

    // ----- ownership --------------------------------------------------------

    function test_ownable2Step_handover() public {
        address newOwner = makeAddr("newOwner");
        vm.prank(owner);
        esc.transferOwnership(newOwner);
        // Two-step: nothing changes until acceptance.
        assertEq(esc.owner(), owner);
        vm.prank(newOwner);
        esc.acceptOwnership();
        assertEq(esc.owner(), newOwner);

        // Old owner has lost its powers; the new owner has them.
        vm.prank(owner);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, owner));
        esc.pause();
        vm.prank(newOwner);
        esc.pause();
        assertTrue(esc.paused());
    }

    // ----- snapshot immunity + conservation ---------------------------------

    /// A setConfig AFTER task creation must not move the task's windows: the
    /// schedule was snapshotted at create.
    function test_setConfig_cannotMoveLiveTaskWindows() public {
        uint64 id = _create(1, false);
        ShillbotEscrow.Config memory cfg = _cfg();
        cfg.challengeWindowSecs = 60;
        cfg.disputeWindowSecs = 1 hours;
        vm.prank(owner);
        esc.setConfig(cfg);

        _claim(id);
        _submit(id);
        _verify(id, VerifyLib.MAX_SCORE);
        assertEq(
            esc.getTask(id).challengeDeadline,
            uint64(block.timestamp) + CHALLENGE_WINDOW,
            "challenge window is the snapshot, not the live config"
        );
        vm.prank(challenger);
        esc.challengeTask{value: BOND}(id);
        assertEq(
            esc.getTask(id).resolutionDeadline,
            uint64(block.timestamp) + DISPUTE_WINDOW,
            "dispute window is the snapshot, not the live config"
        );
    }

    /// After a full mixed run the contract holds exactly the sum of
    /// withdrawable credits; draining them leaves zero — no stranded wei.
    function test_conservation_noStrandedWei() public {
        // Finalized happy path.
        uint64 a = _create(0, false);
        _claim(a);
        _submit(a);
        _verify(a, 600_000);
        // Disputed → default-resolved path.
        uint64 b = _toDisputed();
        // Expired path.
        uint64 c = _create(1, false);

        vm.warp(uint256(esc.getTask(b).resolutionDeadline) + 1);
        esc.finalizeTask(a);
        esc.defaultResolve(b);
        esc.expireTask(c);

        uint256 credited = esc.withdrawable(client) + esc.withdrawable(worker) + esc.withdrawable(treasury)
            + esc.withdrawable(challenger);
        assertEq(address(esc).balance, credited, "balance == sum of credits");

        vm.prank(client);
        esc.withdraw();
        vm.prank(worker);
        esc.withdraw();
        vm.prank(treasury);
        esc.withdraw();
        vm.prank(challenger);
        esc.withdraw();
        assertEq(address(esc).balance, 0, "fully drained, no stranded wei");
    }
}
