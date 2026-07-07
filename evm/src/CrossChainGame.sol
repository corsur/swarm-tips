// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {CertLib} from "./CertLib.sol";
import {PullPayment} from "./base/PullPayment.sol";
import {AttesterGated} from "./base/AttesterGated.sol";
import {ChainTagged} from "./base/ChainTagged.sol";

/// @title CrossChainGame — EVM leg of cross-chain coordination-game matches
/// @notice One contract holds both the per-match native-ETH stake escrows and
///         the operator float pool: tranche locking is atomic with match
///         state (the load-bearing ordering invariant — a certificate without
///         an on-chain lock is inert), and there is a single reentrancy
///         surface. Design + panel-mandated requirements:
///         multichain/decision.md §4.1; reviews in multichain/reviews/.
///
///         Trust model ("no new trust roots"): settlement requires only
///         signatures from keys recorded at escrow time plus deadlines. The
///         operator's pool is on-chain collateral — its insolvency can only
///         pause NEW cross-chain matches (lockTranche fails), never take a
///         player's funds. Settlement and refunds are permissionless and
///         deliberately NOT pausable: players can always exit.
///
/// Match state machine (per leg; mirrored by the Solana XChainMatch PDA):
///
///   None ──createMatch──► Funded ──lockTranche──► Locked ──settle──► Settled
///            │ (player, stake)      (operator)      │   (terminal 3+3-sig)
///            │                                      │
///            │                                      ├─openClaim──► Claiming ──settleClaim──► ClaimSettled
///            │                                      │  (2-sig checkpoint)  │ (window expired)
///            │                                      │      supersede ◄─────┤ (higher stepCount)
///            │                                      │      equivocation ───┤ (proof updates best claim)
///            └─refundNoCert──► RefundedNoCert       │
///              (t > fundDeadline,                   └─refundTimeout──► RefundedTimeout
///               never locked)                         (t > matchDeadline + maxClaimWindow + 2·skew,
///                                                      nothing settled; tranche → poolFree)
contract CrossChainGame is Ownable2Step, PullPayment, Pausable, AttesterGated, ChainTagged {
    using CertLib for CertLib.MatchLiveCert;
    using CertLib for CertLib.Checkpoint;
    using CertLib for CertLib.OutcomeCert;

    enum Status {
        None,
        Funded,
        Locked,
        Claiming,
        Settled,
        ClaimSettled,
        RefundedNoCert,
        RefundedTimeout
    }

    struct Match {
        Status status;
        address player; // local player wallet (payout/refund recipient)
        address sessionKey; // local player's per-match secp key
        address counterSessionKey; // counterparty's session key (cert cross-check)
        bool playerIsP1; // local player's payoff-matrix seat
        bool localEquivocated; // local session key proven to double-sign
        bool counterEquivocated; // counterparty session key proven to double-sign
        uint128 stakeWei;
        uint128 trancheWei; // 0 until locked
        uint64 fundDeadline; // unix
        uint64 matchDeadline; // unix; must match the cert's value
        uint64 lockedAt; // unix; quote-freshness anchor
        uint64 claimWindowEnd; // anchored: matchDeadline + claimWindowSecs (identical on both legs)
        bytes32 matchLiveDigest; // pinned at first claim/settle
        uint8 bestStepCount;
        uint8 bestOutcomeKind;
        uint64 timeoutOpensAt; // refund backstop, snapshotted at lock so setConfig can't pull it earlier
    }

    // CHAIN_TAG (keccak256("eip155:84532") for Base Sepolia, from the chain
    // registry) lives in the ChainTagged base — immutable so a cert for
    // another chain can never execute here.

    mapping(bytes32 => Match) public matches;

    uint256 public poolFree;
    uint256 public poolLocked;
    /// Per-chain tournament pot accumulating homogeneous-outcome forfeits
    /// (the local analog of Tournament.prize_lamports).
    uint256 public prizePoolWei;

    // Pull-payment ledger (M1) — `withdrawable` / `_credit` / `withdraw()`
    // live in the shared PullPayment base: settle/refund CREDIT the player +
    // treasury instead of pushing ETH, so a recipient that reverts on receive
    // can never brick the state transition or strand the locked tranche.
    // Owner-controlled exits (poolWithdraw, withdrawPrizePool) stay push —
    // the owner chooses the address and a revert there is its own.

    address public treasury;
    uint16 public treasurySplitBps; // [2000, 8000], mirrors GlobalConfig bounds
    uint128 public stakeWei; // exact required stake (chain registry value)
    uint128 public maxTrancheWei; // per-match tranche clamp (panel req A3)
    uint32 public maxClaimWindowSecs;
    uint32 public skewMarginSecs;

    uint16 private constant MIN_SPLIT_BPS = 2000;
    uint16 private constant MAX_SPLIT_BPS = 8000;
    uint16 private constant BPS_DENOM = 10000;

    // Config sanity bounds (L1) — reject mis-set stakes/windows that would brick
    // play or over-expose the pool, without constraining the chosen $5 stake.
    uint128 private constant MIN_STAKE_WEI = 0.0001 ether; // dust floor
    uint128 private constant MAX_STAKE_WEI = 100 ether; // fat-finger cap
    uint32 private constant MIN_WINDOW_SECS = 60; // 1 minute
    uint32 private constant MAX_WINDOW_SECS = 30 days;
    uint128 private constant MAX_TRANCHE_MULTIPLE = 10; // maxTranche <= 10x stake

    event MatchCreated(bytes32 indexed matchId, address indexed player, uint128 stakeWei);
    event TrancheLocked(bytes32 indexed matchId, uint128 trancheWei);
    event Settled(bytes32 indexed matchId, uint8 outcomeKind, uint256 toPlayer, uint256 toTreasury);
    event ClaimOpened(bytes32 indexed matchId, uint8 claimedOutcome, uint64 claimWindowEnd);
    event ClaimSuperseded(bytes32 indexed matchId, uint8 stepCount, uint8 claimedOutcome);
    event EquivocationProven(bytes32 indexed matchId, address equivocator, uint8 newBestOutcome);
    event ClaimSettled(bytes32 indexed matchId, uint8 outcomeKind, uint256 toPlayer);
    event Refunded(bytes32 indexed matchId, Status kind, uint256 toPlayer);
    event PoolDeposited(address indexed from, uint256 amount);
    event PoolWithdrawn(uint256 amount);
    event ConfigUpdated();

    error InvalidStatus();
    error BadSignature();
    error CertMismatch();
    error StaleQuote();
    error DeadlineNotReached();
    error DeadlinePassed();
    error PoolInsufficient();
    error TrancheTooLarge();
    error BadOutcome();
    error BadConfig();

    constructor(
        bytes32 chainTag,
        address initialOwner,
        address operatorSigner_,
        address treasury_,
        uint16 treasurySplitBps_,
        uint128 stakeWei_,
        uint128 maxTrancheWei_,
        uint32 maxClaimWindowSecs_,
        uint32 skewMarginSecs_
    ) Ownable(initialOwner) ChainTagged(chainTag) {
        _validateConfig(
            initialOwner,
            operatorSigner_,
            treasury_,
            treasurySplitBps_,
            stakeWei_,
            maxTrancheWei_,
            maxClaimWindowSecs_,
            skewMarginSecs_
        );
        _setAuthorizedSigner(operatorSigner_);
        treasury = treasury_;
        treasurySplitBps = treasurySplitBps_;
        stakeWei = stakeWei_;
        maxTrancheWei = maxTrancheWei_;
        maxClaimWindowSecs = maxClaimWindowSecs_;
        skewMarginSecs = skewMarginSecs_;
    }

    /// @notice Dedicated secp256k1 certificate signer — NOT the owner EOA and
    ///         not the Solana upgrade authority (key-separation requirement).
    ///         Stored in the AttesterGated base.
    function operatorSigner() public view returns (address) {
        return _authorizedSignerAddr();
    }

    /// @dev The single config-bounds gate, shared by the constructor and
    ///      setConfig so the two paths can never accept divergent values.
    function _validateConfig(
        address owner_,
        address operatorSigner_,
        address treasury_,
        uint16 treasurySplitBps_,
        uint128 stakeWei_,
        uint128 maxTrancheWei_,
        uint32 maxClaimWindowSecs_,
        uint32 skewMarginSecs_
    ) private pure {
        if (operatorSigner_ == address(0) || treasury_ == address(0)) revert BadConfig();
        // Key separation (decision.md trust model): the certificate signer must
        // be a dedicated key, never the owner/upgrade authority or the treasury,
        // so operator compromise can't also move governance or treasury funds.
        if (operatorSigner_ == owner_ || operatorSigner_ == treasury_) revert BadConfig();
        if (treasurySplitBps_ < MIN_SPLIT_BPS || treasurySplitBps_ > MAX_SPLIT_BPS) revert BadConfig();
        // Bounds (L1): stake, tranche-vs-stake, and the two windows.
        if (stakeWei_ < MIN_STAKE_WEI || stakeWei_ > MAX_STAKE_WEI) revert BadConfig();
        if (maxTrancheWei_ < stakeWei_ || maxTrancheWei_ > stakeWei_ * MAX_TRANCHE_MULTIPLE) {
            revert BadConfig();
        }
        if (maxClaimWindowSecs_ < MIN_WINDOW_SECS || maxClaimWindowSecs_ > MAX_WINDOW_SECS) revert BadConfig();
        if (skewMarginSecs_ < MIN_WINDOW_SECS || skewMarginSecs_ > MAX_WINDOW_SECS) revert BadConfig();
    }

    // ---------------------------------------------------------------------
    // Match lifecycle — funding and locking
    // ---------------------------------------------------------------------

    /// @notice Player escrows the exact stake for a matchmaker-assigned
    ///         match ID. Pausable (kill switch for NEW matches only).
    function createMatch(
        bytes32 matchId,
        address sessionKey,
        address counterSessionKey,
        bool playerIsP1,
        uint64 fundDeadline,
        uint64 matchDeadline,
        bytes calldata operatorSig
    ) external payable whenNotPaused {
        Match storage m = matches[matchId];
        if (m.status != Status.None) revert InvalidStatus();
        if (msg.value != stakeWei) revert BadConfig();
        if (sessionKey == address(0) || counterSessionKey == address(0)) revert BadConfig();
        // The two seats must be distinct keys: the equivocation logic and
        // the dual-signature cert model assume two independent parties.
        if (sessionKey == counterSessionKey) revert BadConfig();
        if (fundDeadline <= block.timestamp || matchDeadline <= fundDeadline) revert DeadlinePassed();
        // Operator authorization (M4): the matchmaker signs the exact creation —
        // matchId BOUND to msg.sender, session keys, seat, and deadlines — so a
        // griefer can't squat a paired matchId with their own params before the
        // assigned player funds (a squat would force the legit createMatch to
        // revert on the now-occupied id). msg.sender in the digest ties the sig to
        // the one authorized player; it can't be replayed by anyone else.
        bytes32 digest = keccak256(
            abi.encode(
                block.chainid,
                address(this),
                matchId,
                msg.sender,
                sessionKey,
                counterSessionKey,
                playerIsP1,
                fundDeadline,
                matchDeadline
            )
        );
        _requireOperatorSig(digest, operatorSig);

        m.status = Status.Funded;
        m.player = msg.sender;
        m.sessionKey = sessionKey;
        m.counterSessionKey = counterSessionKey;
        m.playerIsP1 = playerIsP1;
        m.stakeWei = uint128(msg.value);
        m.fundDeadline = fundDeadline;
        m.matchDeadline = matchDeadline;
        emit MatchCreated(matchId, msg.sender, uint128(msg.value));
    }

    /// @notice Permissionless tranche lock. Anyone may submit + pay gas; the
    ///         authorization is the operator's match-live signature over the
    ///         cert (the same signature relayed at pairing — no new operator
    ///         action). The locked amount is `cert.legB.tranche`, so the
    ///         operator commits the exact tranche by signing the cert. MUST
    ///         land (and be quorum-verified by clients at the finalized tag)
    ///         before settle, which requires Locked.
    /// @dev Gas is the caller's cost by default; the operator never pays to
    ///      run this. Optional protocol gas subsidy routes through the
    ///      reputation graph program, not an owner wallet.
    function lockTranche(CertLib.MatchLiveCert calldata cert, bytes calldata operatorSig) external whenNotPaused {
        Match storage m = matches[cert.matchId];
        if (m.status != Status.Funded) revert InvalidStatus();

        CertLib.Leg calldata leg = cert.legB; // leg B is ALWAYS the EVM leg
        // Bind the cert to the funded match — identical to _verifyMatchLive
        // EXCEPT leg.tranche and m.lockedAt, which THIS call is what sets.
        if (
            leg.chainTag != CHAIN_TAG || leg.contractId != _contractIdWord()
                || leg.player != bytes32(uint256(uint160(m.player))) || leg.sessionKey != m.sessionKey
                || cert.legA.sessionKey != m.counterSessionKey || leg.stake != m.stakeWei
                || cert.matchDeadline != m.matchDeadline || (cert.aIsP1 == 1) == m.playerIsP1
        ) revert CertMismatch();
        // Quote freshness anchored at lock time (settle later re-checks vs lockedAt).
        if (uint256(cert.quoteTimestamp) + cert.quoteMaxAgeSecs < block.timestamp) revert StaleQuote();
        // Only the operator signature is required — the lock precedes the
        // players' match-live session signatures.
        _requireOperatorSig(cert.matchLiveDigest(), operatorSig);

        uint128 trancheWei = leg.tranche;
        if (trancheWei > maxTrancheWei) revert TrancheTooLarge();
        if (poolFree < trancheWei) revert PoolInsufficient();

        poolFree -= trancheWei;
        poolLocked += trancheWei;
        m.trancheWei = trancheWei;
        m.lockedAt = uint64(block.timestamp);
        // Snapshot the refund backstop from the config in force at lock time, so a
        // later setConfig that lowers maxClaimWindowSecs/skew cannot pull opensAt
        // ahead of an open claim's window (it is always >= matchDeadline +
        // claimWindowSecs + 2*skew for any cert, preserving the no-race inequality).
        m.timeoutOpensAt = uint64(uint256(m.matchDeadline) + maxClaimWindowSecs + 2 * uint256(skewMarginSecs));
        // Snapshot the claim window too (M2): validate cert.claimWindowSecs against
        // the live maxClaimWindowSecs ONCE, here, so a later setConfig that lowers
        // the max can't DoS a valid openClaim/submitEquivocationProof by failing
        // the bound check at claim time.
        m.claimWindowEnd = _claimWindowEnd(m.matchDeadline, cert.claimWindowSecs);
        m.status = Status.Locked;
        emit TrancheLocked(cert.matchId, trancheWei);
    }

    // ---------------------------------------------------------------------
    // Settlement — terminal certificates (instant path)
    // ---------------------------------------------------------------------

    /// @notice Permissionless settlement against a fully co-signed pair:
    ///         match-live cert + outcome cert, each signed by both player
    ///         session keys AND the operator signer (3+3 signatures).
    /// @param liveSigs [legA session, legB session, operator] over the
    ///        match-live digest; ocSigs same order over the outcome digest.
    function settle(
        CertLib.MatchLiveCert calldata cert,
        CertLib.OutcomeCert calldata oc,
        bytes[3] calldata liveSigs,
        bytes[3] calldata ocSigs
    ) external nonReentrant {
        Match storage m = matches[cert.matchId];
        if (m.status != Status.Locked) revert InvalidStatus();

        bytes32 liveDigest = _verifyMatchLive(m, cert, liveSigs);

        // Outcome must bind the exact match-live cert and be terminal, or
        // be an operator-co-signed timeout outcome.
        if (oc.matchLiveDigest != liveDigest || oc.matchId != cert.matchId) revert CertMismatch();
        if (oc.stepCount != CertLib.TERMINAL_STEP_COUNT && !CertLib.isTimeoutKind(oc.outcomeKind)) {
            revert BadOutcome();
        }
        bytes32 ocDigest = oc.outcomeDigest();
        _requireSigner(ocDigest, ocSigs[0], cert.legA.sessionKey);
        _requireSigner(ocDigest, ocSigs[1], cert.legB.sessionKey);
        _requireOperatorSig(ocDigest, ocSigs[2]);

        m.status = Status.Settled;
        m.matchLiveDigest = liveDigest;
        (uint256 toPlayer, uint256 toTreasury) = _execute(m, oc.outcomeKind);
        emit Settled(cert.matchId, oc.outcomeKind, toPlayer, toTreasury);
    }

    // ---------------------------------------------------------------------
    // Settlement — contested/timeout claims (optimistic path)
    // ---------------------------------------------------------------------

    /// @notice Open a claim with the latest both-player-co-signed
    ///         checkpoint when the counterparty stalls. The claimed outcome
    ///         must be exactly what the checkpoint entitles under the
    ///         timeout semantics (committer/revealer wins).
    function openClaim(
        CertLib.MatchLiveCert calldata cert,
        CertLib.Checkpoint calldata cp,
        bytes[3] calldata liveSigs,
        bytes[2] calldata cpSigs
    ) external {
        Match storage m = matches[cert.matchId];
        if (m.status != Status.Locked) revert InvalidStatus();
        // Use the window snapshotted at lock (M2) — never the live config.
        uint64 windowEnd = m.claimWindowEnd;
        // Claims may only be filed inside the window; after it closes the
        // refund backstop takes over.
        if (block.timestamp > windowEnd) revert DeadlinePassed();

        bytes32 liveDigest = _verifyMatchLive(m, cert, liveSigs);
        if (cp.matchLiveDigest != liveDigest) revert CertMismatch();
        _verifyCheckpoint(cert, cp, cpSigs);
        _verifyMatchupBinding(cp, cert.matchupCommitment);

        m.status = Status.Claiming;
        m.matchLiveDigest = liveDigest;
        m.bestStepCount = cp.stepCount;
        m.bestOutcomeKind = CertLib.deriveClaimOutcome(cp);
        emit ClaimOpened(cert.matchId, m.bestOutcomeKind, windowEnd);
    }

    /// @dev The claim window closes at matchDeadline + claimWindowSecs —
    ///      anchored to the match deadline (shared by both legs via the
    ///      cert), NOT to file time, so both legs compute the identical
    ///      window and a claim on one leg can never race a refund on the
    ///      other. claimWindowSecs is bounded by maxClaimWindowSecs so the
    ///      refundTimeout backstop (matchDeadline + maxClaimWindowSecs +
    ///      2·skew) always strictly follows it.
    function _claimWindowEnd(uint64 matchDeadline, uint32 claimWindowSecs) private view returns (uint64) {
        if (claimWindowSecs > maxClaimWindowSecs) revert BadConfig();
        return matchDeadline + claimWindowSecs;
    }

    /// @notice Replace the best claim with a strictly more complete
    ///         checkpoint (reveals supersede commits). Does not extend the
    ///         window.
    function supersedeClaim(
        CertLib.MatchLiveCert calldata cert,
        CertLib.Checkpoint calldata cp,
        bytes[2] calldata cpSigs
    ) external {
        Match storage m = matches[cert.matchId];
        if (m.status != Status.Claiming) revert InvalidStatus();
        if (block.timestamp > m.claimWindowEnd) revert DeadlinePassed();
        if (cert.matchLiveDigest() != m.matchLiveDigest) revert CertMismatch();
        if (cp.matchLiveDigest != m.matchLiveDigest) revert CertMismatch();
        // An equivocation verdict (bestStepCount == max) is final and
        // cannot be superseded by a completeness claim.
        if (m.bestStepCount == type(uint8).max) revert BadOutcome();
        if (cp.stepCount <= m.bestStepCount) revert BadOutcome();
        _verifyCheckpoint(cert, cp, cpSigs);
        _verifyMatchupBinding(cp, cert.matchupCommitment);

        m.bestStepCount = cp.stepCount;
        m.bestOutcomeKind = CertLib.deriveClaimOutcome(cp);
        emit ClaimSuperseded(cert.matchId, cp.stepCount, m.bestOutcomeKind);
    }

    /// @notice Two distinct co-signed checkpoints at the same stepCount
    ///         from the same session key = equivocation. Which party
    ///         equivocated is recorded as an explicit flag, so the verdict
    ///         is order-independent and never inferred from the current
    ///         claim outcome: local-only → local forfeits; counter-only →
    ///         local wins; both → both forfeit. The window is anchored to
    ///         matchDeadline exactly like openClaim, so the cross-leg and
    ///         refund-backstop guarantees hold identically.
    ///         Known limitation (testnet): a counter-equivocation submitted
    ///         in the final block of the window has little time for the
    ///         honest party to respond; a response-window extension is the
    ///         deferred claim-window-tuning item in decision.md §7.
    function submitEquivocationProof(
        CertLib.MatchLiveCert calldata cert,
        CertLib.Checkpoint calldata cpA,
        CertLib.Checkpoint calldata cpB,
        bytes calldata sigA,
        bytes calldata sigB
    ) external {
        Match storage m = matches[cert.matchId];
        if (m.status != Status.Locked && m.status != Status.Claiming) revert InvalidStatus();

        // Bind the cert to this match and bound its window the same way
        // openClaim does — the equivocation proof stands on the duplicate
        // signature, but the deadline/window it implies must be trusted.
        bytes32 liveDigest = cert.matchLiveDigest();
        if (cert.matchDeadline != m.matchDeadline) revert CertMismatch();
        // Snapshotted at lock (M2), immune to a later setConfig.
        uint64 windowEnd = m.claimWindowEnd;
        if (block.timestamp > windowEnd) revert DeadlinePassed();
        if (cpA.matchLiveDigest != liveDigest || cpB.matchLiveDigest != liveDigest) {
            revert CertMismatch();
        }
        if (m.status == Status.Claiming && m.matchLiveDigest != liveDigest) revert CertMismatch();

        if (cpA.stepCount != cpB.stepCount) revert BadOutcome();
        bytes32 digestA = cpA.checkpointDigest();
        bytes32 digestB = cpB.checkpointDigest();
        if (digestA == digestB) revert BadOutcome();

        address signerA = ECDSA.recover(digestA, sigA);
        address signerB = ECDSA.recover(digestB, sigB);
        if (signerA != signerB) revert BadSignature();

        if (signerA == m.sessionKey) {
            m.localEquivocated = true;
        } else if (signerA == m.counterSessionKey) {
            m.counterEquivocated = true;
        } else {
            revert BadSignature();
        }

        m.status = Status.Claiming;
        m.matchLiveDigest = liveDigest;
        // claimWindowEnd already snapshotted at lock (M2).
        // Verdict from flags only — order-independent, never read from the
        // current claim outcome.
        m.bestOutcomeKind = _equivocationOutcome(m.playerIsP1, m.localEquivocated, m.counterEquivocated);
        m.bestStepCount = type(uint8).max; // equivocation verdict is final
        emit EquivocationProven(cert.matchId, signerA, m.bestOutcomeKind);
    }

    /// @dev Deterministic equivocation verdict. local-only → local forfeits
    ///      (counterparty wins this leg); counter-only → local wins; both →
    ///      both forfeit.
    function _equivocationOutcome(bool localIsP1, bool localEq, bool counterEq) private pure returns (uint8) {
        if (localEq && counterEq) return CertLib.TIMEOUT_BOTH_FORFEIT;
        if (localEq) return localIsP1 ? CertLib.TIMEOUT_P2_WINS : CertLib.TIMEOUT_P1_WINS;
        return localIsP1 ? CertLib.TIMEOUT_P1_WINS : CertLib.TIMEOUT_P2_WINS;
    }

    /// @notice Execute the best claim after the window closes.
    function settleClaim(bytes32 matchId) external nonReentrant {
        Match storage m = matches[matchId];
        if (m.status != Status.Claiming) revert InvalidStatus();
        if (block.timestamp <= m.claimWindowEnd) revert DeadlineNotReached();

        m.status = Status.ClaimSettled;
        (uint256 toPlayer,) = _execute(m, m.bestOutcomeKind);
        emit ClaimSettled(matchId, m.bestOutcomeKind, toPlayer);
    }

    // ---------------------------------------------------------------------
    // Refund paths — every non-happy path conserves value
    // ---------------------------------------------------------------------

    /// @notice Match never reached Locked: full stake back after the fund
    ///         deadline. Pool untouched.
    function refundNoCert(bytes32 matchId) external nonReentrant {
        Match storage m = matches[matchId];
        if (m.status != Status.Funded) revert InvalidStatus();
        if (block.timestamp <= m.fundDeadline) revert DeadlineNotReached();

        m.status = Status.RefundedNoCert;
        uint256 amount = m.stakeWei;
        emit Refunded(matchId, Status.RefundedNoCert, amount);
        _credit(m.player, amount);
    }

    /// @notice Backstop after the claim window plus 2×skewMargin (snapshotted at
    ///         lock as m.timeoutOpensAt). Two cases:
    ///         - Locked, no claim ever filed: player made whole, tranche back to
    ///           the pool — any unfiled forfeit is eaten by the operator by design.
    ///         - Claiming: a co-signed claim already proved the outcome, so this
    ///           realizes THAT outcome exactly like settleClaim. It must NOT flat-
    ///           refund the stake, which would strip a winning claimant of the
    ///           tranche they proved and hand it to whoever races the call. Making
    ///           both terminal functions settle identically removes the race.
    function refundTimeout(bytes32 matchId) external nonReentrant {
        Match storage m = matches[matchId];
        if (m.status != Status.Locked && m.status != Status.Claiming) revert InvalidStatus();
        if (block.timestamp <= m.timeoutOpensAt) revert DeadlineNotReached();

        if (m.status == Status.Claiming) {
            m.status = Status.ClaimSettled;
            (uint256 toPlayer,) = _execute(m, m.bestOutcomeKind);
            emit ClaimSettled(matchId, m.bestOutcomeKind, toPlayer);
            return;
        }

        m.status = Status.RefundedTimeout;
        _releaseTranche(m);
        uint256 amount = m.stakeWei;
        emit Refunded(matchId, Status.RefundedTimeout, amount);
        _credit(m.player, amount);
    }

    // ---------------------------------------------------------------------
    // Float pool
    // ---------------------------------------------------------------------

    function poolDeposit() external payable {
        poolFree += msg.value;
        emit PoolDeposited(msg.sender, msg.value);
    }

    /// @notice Withdraw operator float — free balance only; locked tranches
    ///         are player-protected until their match resolves.
    function poolWithdraw(uint256 amount) external onlyOwner nonReentrant {
        if (amount > poolFree) revert PoolInsufficient();
        poolFree -= amount;
        emit PoolWithdrawn(amount);
        _pay(owner(), amount);
    }

    /// @notice Sweep the per-chain tournament pot (homogeneous forfeits) to
    ///         the prize-distribution flow.
    function withdrawPrizePool(address to, uint256 amount) external onlyOwner nonReentrant {
        if (to == address(0)) revert BadConfig();
        if (amount > prizePoolWei) revert PoolInsufficient();
        prizePoolWei -= amount;
        _pay(to, amount);
    }

    function setConfig(
        address operatorSigner_,
        address treasury_,
        uint16 treasurySplitBps_,
        uint128 stakeWei_,
        uint128 maxTrancheWei_,
        uint32 maxClaimWindowSecs_,
        uint32 skewMarginSecs_
    ) external onlyOwner {
        _validateConfig(
            owner(),
            operatorSigner_,
            treasury_,
            treasurySplitBps_,
            stakeWei_,
            maxTrancheWei_,
            maxClaimWindowSecs_,
            skewMarginSecs_
        );
        _setAuthorizedSigner(operatorSigner_);
        treasury = treasury_;
        treasurySplitBps = treasurySplitBps_;
        stakeWei = stakeWei_;
        maxTrancheWei = maxTrancheWei_;
        maxClaimWindowSecs = maxClaimWindowSecs_;
        skewMarginSecs = skewMarginSecs_;
        emit ConfigUpdated();
    }

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }

    // ---------------------------------------------------------------------
    // Internals
    // ---------------------------------------------------------------------

    /// @dev Verify the match-live cert: our leg's terms against locally
    ///      recorded escrow state (existence ≠ agreement — review 01/03
    ///      finding U3), chain/contract domain binding, quote freshness,
    ///      and all three signatures.
    function _verifyMatchLive(Match storage m, CertLib.MatchLiveCert calldata cert, bytes[3] calldata sigs)
        private
        view
        returns (bytes32 liveDigest)
    {
        CertLib.Leg calldata leg = cert.legB; // leg B is ALWAYS the EVM leg
        if (
            leg.chainTag != CHAIN_TAG || leg.contractId != _contractIdWord()
                || leg.player != bytes32(uint256(uint160(m.player))) || leg.sessionKey != m.sessionKey
                || cert.legA.sessionKey != m.counterSessionKey || leg.stake != m.stakeWei || leg.tranche != m.trancheWei
                || cert.matchDeadline != m.matchDeadline || (cert.aIsP1 == 1) == m.playerIsP1
        ) revert CertMismatch();
        // Quote freshness: the rate schedule must have been quoted no
        // earlier than maxAge before the tranche lock anchored the match.
        if (uint256(cert.quoteTimestamp) + cert.quoteMaxAgeSecs < m.lockedAt) revert StaleQuote();

        liveDigest = cert.matchLiveDigest();
        _requireSigner(liveDigest, sigs[0], cert.legA.sessionKey);
        _requireSigner(liveDigest, sigs[1], leg.sessionKey);
        _requireOperatorSig(liveDigest, sigs[2]);
    }

    function _verifyCheckpoint(
        CertLib.MatchLiveCert calldata cert,
        CertLib.Checkpoint calldata cp,
        bytes[2] calldata sigs
    ) private pure {
        bytes32 digest = cp.checkpointDigest();
        _requireSigner(digest, sigs[0], cert.legA.sessionKey);
        _requireSigner(digest, sigs[1], cert.legB.sessionKey);
    }

    /// @dev On a TERMINAL checkpoint the claim payoff reads `matchupType`. This
    ///      path has no operator signature over the outcome, so a colluding pair
    ///      could otherwise co-sign a fabricated `matchupType`. Bind it to the
    ///      matchmaker's commitment exactly as same-chain reveal does:
    ///      sha256(rMatchup) == matchupCommitment and matchupType == rMatchup LSB.
    ///      Non-terminal checkpoints derive timeout outcomes independent of it.
    function _verifyMatchupBinding(CertLib.Checkpoint calldata cp, bytes32 matchupCommitment) private pure {
        if (cp.stepCount != CertLib.TERMINAL_STEP_COUNT) return;
        if (sha256(abi.encodePacked(cp.rMatchup)) != matchupCommitment) revert CertMismatch();
        if (uint8(cp.rMatchup[31]) & 1 != cp.matchupType) revert CertMismatch();
    }

    function _requireSigner(bytes32 digest, bytes calldata sig, address expected) private pure {
        if (ECDSA.recover(digest, sig) != expected) revert BadSignature();
    }

    /// @dev Operator-signature gate — the AttesterGated base holds the key.
    function _requireOperatorSig(bytes32 digest, bytes calldata sig) private view {
        if (!_recoverAndCheck(digest, sig)) revert BadSignature();
    }

    /// @dev Split an amount into the treasury cut and its remainder. The
    ///      remainder (which absorbs the rounding dust) always goes to the
    ///      caller-chosen complementary bucket, so no wei is unaccounted.
    function _treasuryCut(uint256 amount) private view returns (uint256 cut, uint256 remainder) {
        cut = amount * treasurySplitBps / BPS_DENOM;
        remainder = amount - cut;
    }

    /// @dev Per-leg payoff routing — the EVM mirror of resolve_xleg.
    ///      Classifies the local player's result, then splits funds.
    ///      Conservation invariant (asserted before any transfer):
    ///        stake + tranche
    ///          == toPlayer + toTreasury + toPrize + toPoolReimburse + trancheReleased
    ///      i.e. every wei of escrowed stake and locked tranche is routed
    ///      exactly once.
    function _execute(Match storage m, uint8 outcomeKind) private returns (uint256 toPlayer, uint256 toTreasury) {
        uint256 stake = m.stakeWei;
        uint256 tranche = m.trancheWei;

        uint256 toPrize;
        uint256 toPoolReimburse;
        uint256 trancheReleased = tranche; // released unless a win consumes it

        Result r = _classify(outcomeKind, m.playerIsP1);
        if (r == Result.WinAll) {
            // Winner takes own stake + the cross-chain counter-value of the
            // opponent's stake, paid from the locked tranche. NOTE: `tranche`
            // is operator-set (bounded by maxTrancheWei) and the cert only
            // proves the lock and the signed payout schedule agree on it —
            // cross-leg tranche==opponent-stake consistency is an operator
            // responsibility enforced off-chain via the co-signed rate
            // schedule, not an on-chain invariant. The float pool is the
            // loss-bearing party by design (decision.md §4.1).
            toPlayer = stake + tranche;
            trancheReleased = 0;
        } else if (r == Result.LoseAll) {
            // Full local forfeit: treasury cut funds the DAO flywheel, the
            // remainder reimburses the operator pool (whose other-leg float
            // paid the winner).
            (toTreasury, toPoolReimburse) = _treasuryCut(stake);
        } else if (r == Result.KeepStake) {
            toPlayer = stake;
        } else if (r == Result.HalfBack) {
            // Correct in the homogeneous matrix: half the local stake back,
            // the rest forfeits to treasury + local prize pool (same shape
            // as same-chain payoff.rs homogeneous resolution). The odd-stake
            // dust lands in `forfeit` and thence treasury/prize — conserved.
            toPlayer = stake / 2;
            (toTreasury, toPrize) = _treasuryCut(stake - toPlayer);
        } else {
            // ForfeitToTreasury: both-wrong / both-forfeit — whole local
            // stake splits treasury + local prize pool, pool untouched.
            (toTreasury, toPrize) = _treasuryCut(stake);
        }

        assert(stake + tranche == toPlayer + toTreasury + toPrize + toPoolReimburse + trancheReleased);

        // Effects: pool accounting before interactions (CEI).
        poolLocked -= tranche;
        poolFree += trancheReleased + toPoolReimburse;
        prizePoolWei += toPrize;

        // Interactions last — credit, never push (M1): a reverting treasury or
        // player must not block settlement or strand the released tranche.
        if (toTreasury > 0) _credit(treasury, toTreasury);
        if (toPlayer > 0) _credit(m.player, toPlayer);
    }

    enum Result {
        WinAll, // own stake + opponent counter-value (from tranche)
        LoseAll, // full forfeit: treasury + pool reimbursement
        KeepStake, // homogeneous both-correct
        HalfBack, // homogeneous, locally correct
        ForfeitToTreasury // both-wrong / both-forfeit: treasury + prize pool
    }

    /// @dev Map an outcome kind to the LOCAL player's result. Pure — the
    ///      seat (playerIsP1) is the only local context.
    function _classify(uint8 outcomeKind, bool localIsP1) private pure returns (Result) {
        if (outcomeKind == CertLib.HOMOG_BOTH_CORRECT) return Result.KeepStake;
        if (outcomeKind == CertLib.BOTH_WRONG || outcomeKind == CertLib.TIMEOUT_BOTH_FORFEIT) {
            return Result.ForfeitToTreasury;
        }
        if (outcomeKind == CertLib.HOMOG_P1_CORRECT) {
            return localIsP1 ? Result.HalfBack : Result.ForfeitToTreasury;
        }
        if (outcomeKind == CertLib.HOMOG_P2_CORRECT) {
            return localIsP1 ? Result.ForfeitToTreasury : Result.HalfBack;
        }
        // Heterogeneous + timeout winner-takes-all: local seat decides
        // whether this leg pays the winner or eats the forfeit.
        bool p1Wins = outcomeKind == CertLib.HETERO_P1_WINS || outcomeKind == CertLib.TIMEOUT_P1_WINS;
        bool p2Wins = outcomeKind == CertLib.HETERO_P2_WINS || outcomeKind == CertLib.TIMEOUT_P2_WINS;
        if (!p1Wins && !p2Wins) revert BadOutcome();
        bool localWins = (p1Wins && localIsP1) || (p2Wins && !localIsP1);
        return localWins ? Result.WinAll : Result.LoseAll;
    }

    function _releaseTranche(Match storage m) private {
        uint256 tranche = m.trancheWei;
        if (tranche > 0) {
            poolLocked -= tranche;
            poolFree += tranche;
        }
    }

    function _pay(address to, uint256 amount) private {
        (bool ok,) = payable(to).call{value: amount}("");
        require(ok, "transfer failed");
    }

    // Pull-payment credit + withdraw() live in the shared PullPayment base.
}
