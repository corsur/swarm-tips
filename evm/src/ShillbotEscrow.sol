// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {VerifyLib} from "./VerifyLib.sol";
import {PullPayment} from "./base/PullPayment.sol";
import {AttesterGated} from "./base/AttesterGated.sol";
import {ChainTagged} from "./base/ChainTagged.sol";

/// @title ShillbotEscrow — EVM port of the Shillbot task marketplace escrow
/// @notice Native-ETH task escrow with attester-signature verification: a
///         client escrows a budget for a task, a worker claims and submits,
///         and the allow-listed attester's signature over the canonical
///         `VerifyLib` attestation digest is the ONLY trust gate that prices
///         the work (permissionless relay — v = 27/28 signatures from
///         chain-core `cosign::sign_digest_eth`). Payouts are pull-payment
///         (shared `PullPayment` base): every resolution CREDITS recipients,
///         who pull via `withdraw()`.
///
///         Semantics mirror `programs/shillbot` with three deliberate EVM
///         deltas, each documented at its site:
///         - `claimTask` is arms-length for BOTH verification kinds (Solana
///           scopes the self-claim ban to kind 1) — simpler and stricter;
///         - `expireTask` marks the task `Resolved` with a full client refund
///           (Solana closes the Task PDA; there is no account to close here);
///         - both verification kinds route through `verifyTaskAttested`
///           (Solana kind 0 goes through the Switchboard `verify_task` path).
///
///         Pause policy: state-changing entry points are `whenNotPaused`
///         EXCEPT the liveness paths `withdraw()`, `defaultResolve` and
///         `expireTask`, which must work while paused so a pause can never
///         freeze a resolved dispute or strand an expired task's escrow.
///
/// Task state machine (u8 values are the Solana `TaskState` wire values;
/// `DefaultResolved = 8` is appended, never reordered):
///
///   Open(0) ──claimTask──► Claimed(1) ──submitWork──► Submitted(2)
///     │                     │                           │        │
///     │◄────expireTask──────┘ (t > deadline)            │   approveTask (requiresApproval only)
///     │      [→ Resolved(6), escrow → client]           │        ▼
///     │◄────expireTask (t > submittedAt + timeout)──────┤    Approved(7)
///     │                                                 │        │
///     │                     verifyTaskAttested (attester sig)────┘
///     │                                                 ▼
///     │                                          Verified(3) [payment/fee pinned]
///     │                                                 │
///     │              finalizeTask (t > challengeDeadline)──► Finalized(4)
///     │              challengeTask (t < challengeDeadline)─► Disputed(5)
///     │                                                        │
///     │        resolveChallenge (owner, t <= resolutionDeadline)──► Resolved(6)
///     │        defaultResolve (anyone, t > resolutionDeadline)────► DefaultResolved(8)
contract ShillbotEscrow is Ownable2Step, PullPayment, Pausable, AttesterGated, ChainTagged {
    /// u8 values mirror the Solana TaskState enum exactly (append-only).
    enum TaskState {
        Open, // 0
        Claimed, // 1
        Submitted, // 2
        Verified, // 3
        Finalized, // 4
        Disputed, // 5
        Resolved, // 6
        Approved, // 7
        DefaultResolved // 8 (liveness state; Solana has it too — see
        // programs/shillbot/src/instructions/resolve_challenge_default.rs.
        // This was EVM-first; the "Solana adds it next" note is stale.)
    }

    struct Task {
        address client;
        address worker;
        TaskState state;
        uint8 verificationKind; // VerifyLib.KIND_* (0 = metrics, 1 = deterministic)
        bool requiresApproval;
        uint128 escrowWei;
        bytes32 statementCommitment; // sha256 of the canonical statement (kind 1)
        bytes32 policyId; // keccak256 of the policy manifest bytes (kind 1)
        bytes32 contentIdHash; // sha256 of the submitted content id
        bytes32 artifactHash; // sha256 of the submitted artifact
        uint64 deadline; // unix; claim/submit close strictly before it
        uint64 submittedAt; // unix; verification-timeout anchor
        uint64 verifiedAt; // unix
        uint64 challengeDeadline; // verifiedAt + snapshotted challenge window
        uint64 resolutionDeadline; // challenge time + snapshotted dispute window
        uint32 challengeWindowSecs; // timing schedule SNAPSHOTTED at create so a
        uint32 disputeWindowSecs; // later setConfig can't move a live task's
        uint32 verificationTimeoutSecs; // windows (M3-style snapshot)
        uint128 paymentWei; // pinned at verify (S-03: immune to later setConfig)
        uint128 feeWei; // pinned at verify
        address challenger;
        uint128 bondWei;
    }

    /// Owner-tunable protocol parameters, validated as one unit by
    /// `_validateConfig` (single gate for constructor + setConfig).
    struct Config {
        address attesterSigner; // dedicated key: != owner, != treasury, != 0
        address treasury;
        uint16 protocolFeeBps; // [100, 2500]
        uint64 qualityThreshold; // < MAX_SCORE (== MAX silently pays 0 — fail loud)
        uint32 challengeWindowSecs; // [60, 30 days]
        uint32 disputeWindowSecs; // [1 hours, 30 days]
        uint32 verificationTimeoutSecs; // [1 hours, 90 days]
        uint8 challengeBondMultiplier; // [2, 10] — a MULTIPLIER, not bps (the
        // Solana field's `_bps` suffix is a documented misnomer)
        uint16 bondSlashTreasuryBps; // [0, 10000] treasury share of a slashed bond
        uint128 minEscrowWei; // > 0
        uint128 maxEscrowWei; // >= minEscrowWei
    }

    mapping(uint64 => Task) private _tasks;
    uint64 public nextTaskId;

    address public treasury;
    uint16 public protocolFeeBps;
    uint64 public qualityThreshold;
    uint32 public challengeWindowSecs;
    uint32 public disputeWindowSecs;
    uint32 public verificationTimeoutSecs;
    uint8 public challengeBondMultiplier;
    uint16 public bondSlashTreasuryBps;
    uint128 public minEscrowWei;
    uint128 public maxEscrowWei;

    uint16 private constant MIN_FEE_BPS = 100;
    uint16 private constant MAX_FEE_BPS = 2500;
    uint16 private constant BPS_DENOM = 10000;
    uint32 private constant MIN_CHALLENGE_WINDOW_SECS = 60;
    uint32 private constant MIN_DISPUTE_WINDOW_SECS = 1 hours;
    uint32 private constant MAX_WINDOW_SECS = 30 days;
    uint32 private constant MIN_VERIFICATION_TIMEOUT_SECS = 1 hours;
    uint32 private constant MAX_VERIFICATION_TIMEOUT_SECS = 90 days;
    uint8 private constant MIN_BOND_MULTIPLIER = 2;
    uint8 private constant MAX_BOND_MULTIPLIER = 10;

    event TaskCreated(
        uint64 indexed taskId, address indexed client, uint128 escrowWei, uint64 deadline, uint8 verificationKind
    );
    event TaskClaimed(uint64 indexed taskId, address indexed worker);
    event WorkSubmitted(uint64 indexed taskId, bytes32 contentIdHash, bytes32 artifactHash);
    event TaskApproved(uint64 indexed taskId);
    event TaskVerified(uint64 indexed taskId, uint64 score, uint128 paymentWei, uint128 feeWei, bytes32 digest);
    event TaskFinalized(uint64 indexed taskId, address indexed worker, uint128 paymentWei, uint128 feeWei);
    event TaskChallenged(uint64 indexed taskId, address indexed challenger, uint128 bondWei);
    event ChallengeResolved(uint64 indexed taskId, bool challengerWon, uint128 bondSlashed);
    event ChallengeDefaultResolved(uint64 indexed taskId, uint128 paymentWei, uint128 feeWei, uint128 bondReturned);
    event TaskExpired(uint64 indexed taskId, uint8 stateAtExpiry);
    event ConfigUpdated();

    error InvalidStatus();
    error BadSignature();
    error BadConfig();
    error BadKind();
    error BadEscrow();
    error BadCommitment();
    error BadScore();
    error BadBond();
    error NotParticipant();
    error DeadlinePassed();
    error DeadlineNotReached();

    constructor(bytes32 chainTag, address initialOwner, Config memory cfg) Ownable(initialOwner) ChainTagged(chainTag) {
        _validateConfig(initialOwner, cfg);
        _applyConfig(cfg);
    }

    /// @notice The allow-listed attester key (AttesterGated base storage) —
    ///         never the owner and never the treasury (key separation).
    function attesterSigner() public view returns (address) {
        return _authorizedSignerAddr();
    }

    function getTask(uint64 taskId) external view returns (Task memory) {
        return _tasks[taskId];
    }

    function setConfig(Config calldata cfg) external onlyOwner {
        _validateConfig(owner(), cfg);
        _applyConfig(cfg);
        emit ConfigUpdated();
    }

    /// @dev Single config-bounds gate shared by the constructor and setConfig.
    function _validateConfig(address owner_, Config memory cfg) private pure {
        if (cfg.attesterSigner == address(0) || cfg.treasury == address(0)) revert BadConfig();
        // Key separation: the attester key never doubles as governance or payee.
        if (cfg.attesterSigner == owner_ || cfg.attesterSigner == cfg.treasury) revert BadConfig();
        if (cfg.protocolFeeBps < MIN_FEE_BPS || cfg.protocolFeeBps > MAX_FEE_BPS) revert BadConfig();
        // Fail loud: threshold == MAX_SCORE makes every payout silently 0 on
        // Solana (verified load-bearing fact) — rejected outright here.
        if (cfg.qualityThreshold >= VerifyLib.MAX_SCORE) revert BadConfig();
        if (cfg.challengeWindowSecs < MIN_CHALLENGE_WINDOW_SECS || cfg.challengeWindowSecs > MAX_WINDOW_SECS) {
            revert BadConfig();
        }
        if (cfg.disputeWindowSecs < MIN_DISPUTE_WINDOW_SECS || cfg.disputeWindowSecs > MAX_WINDOW_SECS) {
            revert BadConfig();
        }
        if (
            cfg.verificationTimeoutSecs < MIN_VERIFICATION_TIMEOUT_SECS
                || cfg.verificationTimeoutSecs > MAX_VERIFICATION_TIMEOUT_SECS
        ) revert BadConfig();
        if (cfg.challengeBondMultiplier < MIN_BOND_MULTIPLIER || cfg.challengeBondMultiplier > MAX_BOND_MULTIPLIER) {
            revert BadConfig();
        }
        if (cfg.bondSlashTreasuryBps > BPS_DENOM) revert BadConfig();
        if (cfg.minEscrowWei == 0 || cfg.maxEscrowWei < cfg.minEscrowWei) revert BadConfig();
    }

    function _applyConfig(Config memory cfg) private {
        _setAuthorizedSigner(cfg.attesterSigner);
        treasury = cfg.treasury;
        protocolFeeBps = cfg.protocolFeeBps;
        qualityThreshold = cfg.qualityThreshold;
        challengeWindowSecs = cfg.challengeWindowSecs;
        disputeWindowSecs = cfg.disputeWindowSecs;
        verificationTimeoutSecs = cfg.verificationTimeoutSecs;
        challengeBondMultiplier = cfg.challengeBondMultiplier;
        bondSlashTreasuryBps = cfg.bondSlashTreasuryBps;
        minEscrowWei = cfg.minEscrowWei;
        maxEscrowWei = cfg.maxEscrowWei;
    }

    // ---------------------------------------------------------------------
    // Task lifecycle
    // ---------------------------------------------------------------------

    /// @notice Create a task by escrowing the exact budget (msg.value).
    ///         Deterministic tasks (kind 1) must carry the statement
    ///         commitment + policy id the attester certifies against.
    function createTask(
        bytes32 statementCommitment,
        bytes32 policyId,
        uint8 verificationKind,
        uint64 deadline,
        bool requiresApproval
    ) external payable whenNotPaused returns (uint64 taskId) {
        // Checks
        if (verificationKind > VerifyLib.KIND_DETERMINISTIC_ATTESTED) revert BadKind();
        if (msg.value < minEscrowWei || msg.value > maxEscrowWei) revert BadEscrow();
        if (deadline <= block.timestamp) revert DeadlinePassed();
        if (verificationKind == VerifyLib.KIND_DETERMINISTIC_ATTESTED) {
            if (statementCommitment == bytes32(0) || policyId == bytes32(0)) revert BadCommitment();
        }

        // Effects
        taskId = nextTaskId;
        nextTaskId = taskId + 1;
        Task storage t = _tasks[taskId];
        t.client = msg.sender;
        t.state = TaskState.Open;
        t.verificationKind = verificationKind;
        t.requiresApproval = requiresApproval;
        t.escrowWei = uint128(msg.value); // safe: msg.value <= maxEscrowWei (uint128)
        t.statementCommitment = statementCommitment;
        t.policyId = policyId;
        t.deadline = deadline;
        // Freeze this task's timing schedule at the config in force now, so a
        // later setConfig can never move a live task's windows.
        t.challengeWindowSecs = challengeWindowSecs;
        t.disputeWindowSecs = disputeWindowSecs;
        t.verificationTimeoutSecs = verificationTimeoutSecs;
        emit TaskCreated(taskId, msg.sender, uint128(msg.value), deadline, verificationKind);
    }

    /// @notice Claim an open task. Arms-length for BOTH kinds: the client can
    ///         never claim its own task (stricter than Solana, which scopes
    ///         the self-claim ban to kind 1 — a client paying itself is never
    ///         useful, so the simpler blanket rule wins on EVM).
    function claimTask(uint64 taskId) external whenNotPaused {
        Task storage t = _existing(taskId);
        if (t.state != TaskState.Open) revert InvalidStatus();
        if (msg.sender == t.client) revert NotParticipant();
        // Strict `<`: at the deadline second a task is neither claimable nor
        // yet expirable (expireTask opens strictly after) — dead-second parity
        // with the Solana program.
        if (block.timestamp >= t.deadline) revert DeadlinePassed();

        t.worker = msg.sender;
        t.state = TaskState.Claimed;
        emit TaskClaimed(taskId, msg.sender);
    }

    /// @notice Submit the completed work: the content id hash and the
    ///         artifact hash the attestation digest binds. Worker only,
    ///         before the deadline (Solana parity, submit_margin = 0).
    function submitWork(uint64 taskId, bytes32 contentIdHash, bytes32 artifactHash) external whenNotPaused {
        Task storage t = _existing(taskId);
        if (t.state != TaskState.Claimed) revert InvalidStatus();
        if (msg.sender != t.worker) revert NotParticipant();
        if (block.timestamp >= t.deadline) revert DeadlinePassed();
        // Both hashes are commitments the attestation digest binds; a zero
        // value could never verify — reject at the boundary.
        if (contentIdHash == bytes32(0) || artifactHash == bytes32(0)) revert BadCommitment();

        t.contentIdHash = contentIdHash;
        t.artifactHash = artifactHash;
        t.submittedAt = uint64(block.timestamp);
        t.state = TaskState.Submitted;
        emit WorkSubmitted(taskId, contentIdHash, artifactHash);
    }

    /// @notice Client sign-off for tasks created with `requiresApproval`.
    ///         Verification requires Approved for those tasks.
    function approveTask(uint64 taskId) external whenNotPaused {
        Task storage t = _existing(taskId);
        if (t.state != TaskState.Submitted) revert InvalidStatus();
        if (!t.requiresApproval) revert InvalidStatus();
        if (msg.sender != t.client) revert NotParticipant();

        t.state = TaskState.Approved;
        emit TaskApproved(taskId);
    }

    /// @notice Price the work from the attester's signed score. PERMISSIONLESS
    ///         relay — the signature over the canonical VerifyLib attestation
    ///         digest (bound to CHAIN_TAG, this contract, the task id, the
    ///         task's descriptor, and the score) is the only trust gate.
    ///         kind 1 admits only {0, MAX_SCORE}; kind 0 admits the continuum
    ///         and prices via the Solana `compute_payment` formula. Payment and
    ///         fee are PINNED here so later setConfig can't reprice (S-03).
    function verifyTaskAttested(uint64 taskId, uint64 score, bytes calldata sig) external whenNotPaused {
        Task storage t = _existing(taskId);
        // Checks
        TaskState required = t.requiresApproval ? TaskState.Approved : TaskState.Submitted;
        if (t.state != required) revert InvalidStatus();
        // Arms-length (Solana ArmsLengthViolation): the attester key must not
        // be the worker it is pricing.
        if (t.worker == attesterSigner()) revert NotParticipant();
        if (t.verificationKind == VerifyLib.KIND_DETERMINISTIC_ATTESTED) {
            if (score != 0 && score != VerifyLib.MAX_SCORE) revert BadScore();
        } else if (score > VerifyLib.MAX_SCORE) {
            revert BadScore();
        }
        bytes32 digest = VerifyLib.attestationDigest(
            CHAIN_TAG,
            _contractIdWord(),
            taskId,
            VerifyLib.Descriptor({
                verificationKind: t.verificationKind,
                statementCommitment: t.statementCommitment,
                policyId: t.policyId,
                artifactHash: t.artifactHash
            }),
            score
        );
        if (!_recoverAndCheck(digest, sig)) revert BadSignature();
        (uint128 payment, uint128 fee) = _computePayment(score, qualityThreshold, t.escrowWei, protocolFeeBps);

        // Effects
        t.paymentWei = payment;
        t.feeWei = fee;
        t.verifiedAt = uint64(block.timestamp);
        t.challengeDeadline = uint64(block.timestamp) + t.challengeWindowSecs;
        t.state = TaskState.Verified;
        emit TaskVerified(taskId, score, payment, fee, digest);
    }

    /// @notice Post the exact bond to dispute a verified task while the
    ///         challenge window is open. Strict `<`: the boundary second
    ///         belongs to neither challenge nor finalize (Solana parity).
    ///         ALWAYS arms the resolution deadline, so an unadjudicated
    ///         dispute can never freeze funds (defaultResolve backstop).
    function challengeTask(uint64 taskId) external payable whenNotPaused {
        Task storage t = _existing(taskId);
        if (t.state != TaskState.Verified) revert InvalidStatus();
        if (block.timestamp >= t.challengeDeadline) revert DeadlinePassed();
        // Bond from the LIVE multiplier (Solana parity: computed at challenge
        // time), exact-value only.
        uint256 bond = uint256(t.escrowWei) * challengeBondMultiplier;
        if (bond > type(uint128).max) revert BadBond();
        if (msg.value != bond) revert BadBond();

        t.challenger = msg.sender;
        t.bondWei = uint128(bond);
        t.resolutionDeadline = uint64(block.timestamp) + t.disputeWindowSecs;
        t.state = TaskState.Disputed;
        emit TaskChallenged(taskId, msg.sender, uint128(bond));
    }

    /// @notice Owner adjudication, only WITHIN the dispute window (inclusive
    ///         `<=`; strictly after it, adjudication passes to the
    ///         permissionless `defaultResolve`). Challenger won: escrow home
    ///         to the client, bond back. Worker won: pinned payout executes
    ///         and the bond is slashed worker/treasury per
    ///         `bondSlashTreasuryBps`.
    function resolveChallenge(uint64 taskId, bool challengerWon) external onlyOwner whenNotPaused {
        Task storage t = _existing(taskId);
        if (t.state != TaskState.Disputed) revert InvalidStatus();
        if (block.timestamp > t.resolutionDeadline) revert DeadlinePassed();

        uint256 escrow = t.escrowWei;
        uint256 bond = t.bondWei;

        // Effects
        t.state = TaskState.Resolved;
        uint128 bondSlashed = 0;
        if (challengerWon) {
            // Conservation: escrow + bond, credited exactly once each.
            _credit(t.client, escrow);
            _credit(t.challenger, bond);
        } else {
            uint256 payment = t.paymentWei;
            uint256 fee = t.feeWei;
            uint256 remainder = escrow - payment - fee; // checked: reverts on underflow
            uint256 bondToTreasury = (bond * bondSlashTreasuryBps) / BPS_DENOM;
            uint256 bondToWorker = bond - bondToTreasury;
            // Conservation over both pots before crediting.
            assert(payment + fee + remainder == escrow);
            assert(bondToWorker + bondToTreasury == bond);
            _credit(t.worker, payment + bondToWorker);
            _credit(treasury, fee + bondToTreasury);
            _credit(t.client, remainder);
            bondSlashed = uint128(bond);
        }
        emit ChallengeResolved(taskId, challengerWon, bondSlashed);
    }

    /// @notice Permissionless liveness backstop (NOT pausable): once the
    ///         dispute window lapses without owner adjudication, the pinned
    ///         payout executes (worker-favoring — the task WAS verified) and
    ///         the bond returns UN-SLASHED (no adjudication happened). A
    ///         challenge is therefore a bounded delay, never a freeze and
    ///         never a grief profit.
    function defaultResolve(uint64 taskId) external {
        Task storage t = _existing(taskId);
        if (t.state != TaskState.Disputed) revert InvalidStatus();
        // Strict `>` — the boundary second still belongs to the owner.
        if (block.timestamp <= t.resolutionDeadline) revert DeadlineNotReached();

        uint256 escrow = t.escrowWei;
        uint256 payment = t.paymentWei;
        uint256 fee = t.feeWei;
        uint256 bond = t.bondWei;
        uint256 remainder = escrow - payment - fee; // checked: reverts on underflow
        assert(payment + fee + remainder == escrow);

        // Effects
        t.state = TaskState.DefaultResolved;
        _credit(t.worker, payment);
        _credit(treasury, fee);
        _credit(t.client, remainder);
        _credit(t.challenger, bond);
        emit ChallengeDefaultResolved(taskId, uint128(payment), uint128(fee), uint128(bond));
    }

    /// @notice Permissionless payout crank once the challenge window closes
    ///         unchallenged (strict `>`, Solana parity). Executes the PINNED
    ///         payment/fee from verification time — immune to any setConfig
    ///         in between (S-03).
    function finalizeTask(uint64 taskId) external whenNotPaused {
        Task storage t = _existing(taskId);
        if (t.state != TaskState.Verified) revert InvalidStatus();
        if (block.timestamp <= t.challengeDeadline) revert DeadlineNotReached();

        uint256 escrow = t.escrowWei;
        uint256 payment = t.paymentWei;
        uint256 fee = t.feeWei;
        uint256 remainder = escrow - payment - fee; // checked: reverts on underflow
        assert(payment + fee + remainder == escrow);

        // Effects
        t.state = TaskState.Finalized;
        _credit(t.worker, payment);
        _credit(treasury, fee);
        _credit(t.client, remainder);
        emit TaskFinalized(taskId, t.worker, uint128(payment), uint128(fee));
    }

    /// @notice Permissionless expiry crank (NOT pausable — a pause must not
    ///         strand a dead task's escrow): Open/Claimed strictly past the
    ///         deadline, or Submitted/Approved strictly past submittedAt +
    ///         the snapshotted verification timeout. Full escrow back to the
    ///         client. Solana CLOSES the Task PDA here; the EVM equivalent is
    ///         the terminal `Resolved` state with zero payment.
    function expireTask(uint64 taskId) external {
        Task storage t = _existing(taskId);
        TaskState stateAtExpiry = t.state;
        if (stateAtExpiry == TaskState.Open || stateAtExpiry == TaskState.Claimed) {
            if (block.timestamp <= t.deadline) revert DeadlineNotReached();
        } else if (stateAtExpiry == TaskState.Submitted || stateAtExpiry == TaskState.Approved) {
            // Approval does not reset the clock (Solana parity): the timeout
            // anchors on submittedAt for both states.
            if (block.timestamp <= uint256(t.submittedAt) + t.verificationTimeoutSecs) {
                revert DeadlineNotReached();
            }
        } else {
            revert InvalidStatus();
        }

        uint256 escrow = t.escrowWei;
        // Effects — full refund, nothing pinned to pay out.
        assert(t.paymentWei == 0 && t.feeWei == 0);
        t.state = TaskState.Resolved;
        _credit(t.client, escrow);
        emit TaskExpired(taskId, uint8(stateAtExpiry));
    }

    // ---------------------------------------------------------------------
    // Internals
    // ---------------------------------------------------------------------

    /// @dev Created tasks always have a nonzero client (msg.sender); a zero
    ///      client means the id was never issued — reject at the boundary so
    ///      cranks can't mint events/state for phantom tasks.
    function _existing(uint64 taskId) private view returns (Task storage t) {
        t = _tasks[taskId];
        if (t.client == address(0)) revert InvalidStatus();
    }

    /// @dev The Solana `scoring::compute_payment` formula, verbatim:
    ///        score < threshold            → (0, 0)
    ///        gross = escrow * (score - threshold) / (MAX_SCORE - threshold)
    ///        fee   = gross * feeBps / 10_000; payment = gross - fee
    ///      The divisor is nonzero because config forbids threshold ==
    ///      MAX_SCORE. gross <= escrow (score <= MAX_SCORE), so the uint128
    ///      casts never truncate. For kind 1 the caller restricts score to
    ///      {0, MAX_SCORE}, which this formula prices as (0,0) /
    ///      full-escrow-minus-fee — exactly the binary payout.
    function _computePayment(uint64 score, uint64 threshold, uint128 escrow, uint16 feeBps)
        private
        pure
        returns (uint128 payment, uint128 fee)
    {
        // Preconditions (caller-gated, re-asserted here).
        assert(score <= VerifyLib.MAX_SCORE);
        assert(threshold < VerifyLib.MAX_SCORE);
        if (score < threshold) return (0, 0);
        uint256 range = uint256(VerifyLib.MAX_SCORE) - threshold;
        uint256 gross = (uint256(escrow) * (score - threshold)) / range;
        uint256 feeAmt = (gross * feeBps) / BPS_DENOM;
        uint256 pay = gross - feeAmt;
        // Postcondition: conservation over the escrow.
        assert(pay + feeAmt <= escrow);
        return (uint128(pay), uint128(feeAmt));
    }

    // ---------------------------------------------------------------------
    // Owner operations
    // ---------------------------------------------------------------------

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }
}
