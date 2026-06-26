// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {CertLib} from "./CertLib.sol";

/// @title CoordinationGame — same-chain (EVM-vs-EVM) 1v1 commit-reveal game
/// @notice The Solidity port of `programs/coordination-game`: two players stake
///         the SAME native amount into one contract, commit a hidden guess,
///         reveal, and the pot (2× stake) is paid out by the canonical payoff
///         matrix. Unlike CrossChainGame there is no operator float pool — both
///         stakes are escrowed here, so the winner is paid directly from the
///         pot and no cross-chain certificate is involved. The payoff is derived
///         by the SAME `CertLib` functions the cross-chain cert and the Solana
///         program use, so all three agree by construction.
///
///         Trust model: the matchmaker (operatorSigner) only attests that a
///         match's `matchupCommitment` is legitimate at creation — it never
///         touches funds and cannot change an outcome. Reveal and timeout
///         resolution are permissionless: players can always exit.
///
/// State machine (mirrors the Solana Game PDA):
///
///   None ──createGame──► Pending ──joinGame──► Active
///          (p1, stake,     │ (operator-       │ (p2, stake)
///           commitment)    │  attested)       │
///                          │                  ├─commitGuess(first)──► Committing
///                          │                  └─resolveTimeout──► Resolved [both forfeit]
///   Committing ─commitGuess(second)─► Revealing
///   Committing ─resolveTimeout──► Resolved [committer wins]
///   Revealing  ─revealGuess(both)──► Resolved [payoff matrix]
///   Revealing  ─resolveTimeout──► Resolved [revealer wins / both forfeit]
///   Resolved   ─(terminal)
contract CoordinationGame is Ownable2Step, ReentrancyGuard, Pausable {
    enum Status {
        None,
        Pending,
        Active,
        Committing,
        Revealing,
        Resolved
    }

    struct Game {
        Status status;
        address player1; // creator
        address player2; // joiner
        uint8 p1Guess; // 255 = unrevealed
        uint8 p2Guess; // 255 = unrevealed
        uint8 firstCommitter; // 1 | 2; 255 = unset
        uint8 matchupType; // 0 | 1; 255 = unset (revealed by the first revealer)
        uint128 stakeWei;
        bytes32 matchupCommitment;
        bytes32 p1Commit; // 0 = not committed
        bytes32 p2Commit;
        uint64 activatedAt; // join time — Active/commit-timeout anchor
        uint64 firstCommitAt; // first commit time — Committing-timeout anchor
        uint64 bothCommitAt; // second commit time — Revealing-timeout anchor
    }

    mapping(bytes32 => Game) public games;

    /// Accumulates homogeneous-outcome forfeits — the local analog of
    /// Tournament.prize_lamports; the owner sweeps it to the tournament pot.
    uint256 public prizePoolWei;

    /// Dedicated secp256k1 matchmaker key — NOT the owner and not the treasury
    /// (key separation: matchmaker compromise can't move governance/treasury).
    address public operatorSigner;
    address public treasury;
    uint16 public treasurySplitBps; // [2000, 8000], mirrors GlobalConfig bounds
    uint128 public stakeWei; // exact required stake
    uint32 public commitTimeoutSecs; // Active + Committing stages
    uint32 public revealTimeoutSecs; // Revealing stage

    uint8 private constant UNREVEALED = 255;
    uint8 private constant MATCHUP_UNSET = 255;
    uint16 private constant MIN_SPLIT_BPS = 2000;
    uint16 private constant MAX_SPLIT_BPS = 8000;
    uint16 private constant BPS_DENOM = 10000;

    event GameCreated(bytes32 indexed gameId, address indexed player1, uint128 stakeWei);
    event GameStarted(bytes32 indexed gameId, address indexed player1, address indexed player2);
    event GuessCommitted(bytes32 indexed gameId, address indexed player);
    event GuessRevealed(bytes32 indexed gameId, address indexed player);
    event GameResolved(bytes32 indexed gameId, uint8 outcomeKind, uint256 toP1, uint256 toP2, uint256 toTreasury);
    event PrizePoolWithdrawn(address indexed to, uint256 amount);
    event ConfigUpdated();

    error InvalidStatus();
    error BadSignature();
    error BadStake();
    error BadCommitment();
    error NotParticipant();
    error AlreadyActed();
    error CertMismatch();
    error DeadlineNotReached();
    error BadOutcome();
    error BadConfig();

    constructor(
        address initialOwner,
        address operatorSigner_,
        address treasury_,
        uint16 treasurySplitBps_,
        uint128 stakeWei_,
        uint32 commitTimeoutSecs_,
        uint32 revealTimeoutSecs_
    ) Ownable(initialOwner) {
        _validateConfig(
            initialOwner,
            operatorSigner_,
            treasury_,
            treasurySplitBps_,
            stakeWei_,
            commitTimeoutSecs_,
            revealTimeoutSecs_
        );
        operatorSigner = operatorSigner_;
        treasury = treasury_;
        treasurySplitBps = treasurySplitBps_;
        stakeWei = stakeWei_;
        commitTimeoutSecs = commitTimeoutSecs_;
        revealTimeoutSecs = revealTimeoutSecs_;
    }

    /// @dev Single config-bounds gate shared by the constructor and setConfig.
    function _validateConfig(
        address owner_,
        address operatorSigner_,
        address treasury_,
        uint16 treasurySplitBps_,
        uint128 stakeWei_,
        uint32 commitTimeoutSecs_,
        uint32 revealTimeoutSecs_
    ) private pure {
        if (operatorSigner_ == address(0) || treasury_ == address(0) || stakeWei_ == 0) {
            revert BadConfig();
        }
        // Key separation: the matchmaker key is never the owner or the treasury.
        if (operatorSigner_ == owner_ || operatorSigner_ == treasury_) revert BadConfig();
        if (treasurySplitBps_ < MIN_SPLIT_BPS || treasurySplitBps_ > MAX_SPLIT_BPS) revert BadConfig();
        if (commitTimeoutSecs_ == 0 || revealTimeoutSecs_ == 0) revert BadConfig();
    }

    function setConfig(
        address operatorSigner_,
        address treasury_,
        uint16 treasurySplitBps_,
        uint128 stakeWei_,
        uint32 commitTimeoutSecs_,
        uint32 revealTimeoutSecs_
    ) external onlyOwner {
        _validateConfig(
            owner(), operatorSigner_, treasury_, treasurySplitBps_, stakeWei_, commitTimeoutSecs_, revealTimeoutSecs_
        );
        operatorSigner = operatorSigner_;
        treasury = treasury_;
        treasurySplitBps = treasurySplitBps_;
        stakeWei = stakeWei_;
        commitTimeoutSecs = commitTimeoutSecs_;
        revealTimeoutSecs = revealTimeoutSecs_;
        emit ConfigUpdated();
    }

    // ---------------------------------------------------------------------
    // Game lifecycle
    // ---------------------------------------------------------------------

    /// @notice Create a game by escrowing the exact stake. `operatorSig` is the
    ///         matchmaker's attestation that `matchupCommitment` is legitimate
    ///         (the EVM analog of the Solana matchmaker co-signing create_game).
    function createGame(bytes32 gameId, bytes32 matchupCommitment, bytes calldata operatorSig)
        external
        payable
        whenNotPaused
    {
        Game storage g = games[gameId];
        if (g.status != Status.None) revert InvalidStatus();
        if (msg.value != stakeWei) revert BadStake();
        if (matchupCommitment == bytes32(0)) revert BadCommitment();
        bytes32 digest = keccak256(abi.encode(block.chainid, address(this), gameId, matchupCommitment));
        if (ECDSA.recover(digest, operatorSig) != operatorSigner) revert BadSignature();

        g.status = Status.Pending;
        g.player1 = msg.sender;
        g.stakeWei = uint128(msg.value);
        g.matchupCommitment = matchupCommitment;
        g.p1Guess = UNREVEALED;
        g.p2Guess = UNREVEALED;
        g.firstCommitter = 0;
        g.matchupType = MATCHUP_UNSET;
        emit GameCreated(gameId, msg.sender, g.stakeWei);
    }

    /// @notice Join a pending game by matching the stake; the game goes Active.
    function joinGame(bytes32 gameId) external payable whenNotPaused {
        Game storage g = games[gameId];
        if (g.status != Status.Pending) revert InvalidStatus();
        if (msg.sender == g.player1) revert NotParticipant();
        if (msg.value != g.stakeWei) revert BadStake();

        g.player2 = msg.sender;
        g.status = Status.Active;
        g.activatedAt = uint64(block.timestamp);
        emit GameStarted(gameId, g.player1, msg.sender);
    }

    /// @notice Commit SHA-256(r) of a hidden guess preimage. First commit moves
    ///         the game to Committing and records the first committer (the
    ///         heterogeneous-tie-break seat); second commit moves it to Revealing.
    function commitGuess(bytes32 gameId, bytes32 commitment) external {
        Game storage g = games[gameId];
        if (g.status != Status.Active && g.status != Status.Committing) revert InvalidStatus();
        if (commitment == bytes32(0)) revert BadCommitment();
        bool isP1 = msg.sender == g.player1;
        if (!isP1 && msg.sender != g.player2) revert NotParticipant();

        if (isP1) {
            if (g.p1Commit != bytes32(0)) revert AlreadyActed();
            g.p1Commit = commitment;
        } else {
            if (g.p2Commit != bytes32(0)) revert AlreadyActed();
            g.p2Commit = commitment;
        }

        if (g.firstCommitter == 0) {
            g.firstCommitter = isP1 ? 1 : 2;
            g.firstCommitAt = uint64(block.timestamp);
            g.status = Status.Committing;
        } else {
            g.bothCommitAt = uint64(block.timestamp);
            g.status = Status.Revealing;
        }
        emit GuessCommitted(gameId, msg.sender);
    }

    /// @notice Reveal a guess preimage. The first revealer also reveals the
    ///         matchup type by providing `rMatchup` (bound to the commitment);
    ///         the second revealer passes bytes32(0). Resolves on both reveals.
    function revealGuess(bytes32 gameId, bytes32 r, bytes32 rMatchup) external nonReentrant {
        Game storage g = games[gameId];
        if (g.status != Status.Revealing) revert InvalidStatus();
        bool isP1 = msg.sender == g.player1;
        if (!isP1 && msg.sender != g.player2) revert NotParticipant();

        bytes32 commit = isP1 ? g.p1Commit : g.p2Commit;
        if (sha256(abi.encodePacked(r)) != commit) revert CertMismatch();
        uint8 guess = uint8(r[31]) & 1;

        if (isP1) {
            if (g.p1Guess != UNREVEALED) revert AlreadyActed();
            g.p1Guess = guess;
        } else {
            if (g.p2Guess != UNREVEALED) revert AlreadyActed();
            g.p2Guess = guess;
        }

        if (g.matchupType == MATCHUP_UNSET) {
            // First revealer binds the matchup type to the matchmaker commitment.
            if (sha256(abi.encodePacked(rMatchup)) != g.matchupCommitment) revert CertMismatch();
            g.matchupType = uint8(rMatchup[31]) & 1;
        } else if (rMatchup != bytes32(0)) {
            revert CertMismatch(); // second revealer must pass the null sentinel
        }
        emit GuessRevealed(gameId, msg.sender);

        if (g.p1Guess != UNREVEALED && g.p2Guess != UNREVEALED) {
            _resolve(gameId, g, CertLib.deriveTerminalOutcome(_checkpoint(g, CertLib.TERMINAL_STEP_COUNT)));
        }
    }

    /// @notice Permissionless crank: resolve a game whose current stage has
    ///         timed out. Active/Committing use commitTimeoutSecs, Revealing uses
    ///         revealTimeoutSecs. Outcome is the canonical claim-timeout result.
    function resolveTimeout(bytes32 gameId) external nonReentrant {
        Game storage g = games[gameId];
        uint8 stepCount;
        if (g.status == Status.Active) {
            _requireElapsed(g.activatedAt, commitTimeoutSecs);
            stepCount = 0;
        } else if (g.status == Status.Committing) {
            _requireElapsed(g.firstCommitAt, commitTimeoutSecs);
            stepCount = 1;
        } else if (g.status == Status.Revealing) {
            _requireElapsed(g.bothCommitAt, revealTimeoutSecs);
            stepCount = 3;
        } else {
            revert InvalidStatus();
        }
        _resolve(gameId, g, CertLib.deriveClaimOutcome(_checkpoint(g, stepCount)));
    }

    function _requireElapsed(uint64 anchor, uint32 window) private view {
        if (block.timestamp < uint256(anchor) + window) revert DeadlineNotReached();
    }

    // ---------------------------------------------------------------------
    // Resolution
    // ---------------------------------------------------------------------

    /// @dev Build the in-memory checkpoint the CertLib derivations read. Only
    ///      the payoff-relevant fields are populated; encoding fields are unused
    ///      by derive* and left zero.
    function _checkpoint(Game storage g, uint8 stepCount) private view returns (CertLib.Checkpoint memory cp) {
        cp.stepCount = stepCount;
        cp.p1Guess = g.p1Guess;
        cp.p2Guess = g.p2Guess;
        cp.firstCommitter = g.firstCommitter;
        cp.matchupType = g.matchupType;
    }

    /// @dev Map an outcome kind to the pot split (p1, p2, tournament gain) and
    ///      pay out. The pot is exactly 2× stake; conservation is asserted before
    ///      any transfer (CEI: status + prize-pool effects, then interactions).
    function _resolve(bytes32 gameId, Game storage g, uint8 outcomeKind) private {
        uint256 stake = g.stakeWei;
        (uint256 toP1, uint256 toP2, uint256 gain) = _amounts(outcomeKind, stake);
        assert(toP1 + toP2 + gain == stake * 2);

        (uint256 toTreasury, uint256 toPrize) = _treasuryCut(gain);
        assert(toTreasury + toPrize == gain);

        // Effects.
        g.status = Status.Resolved;
        prizePoolWei += toPrize;

        // Interactions.
        if (toTreasury > 0) _pay(treasury, toTreasury);
        if (toP1 > 0) _pay(g.player1, toP1);
        if (toP2 > 0) _pay(g.player2, toP2);
        emit GameResolved(gameId, outcomeKind, toP1, toP2, toTreasury);
    }

    /// @dev The canonical payoff matrix as pot amounts — the EVM mirror of
    ///      payoff.rs::resolve_game, keyed by CertLib's outcome kind so it can
    ///      never disagree with the derivation.
    function _amounts(uint8 kind, uint256 stake) private pure returns (uint256 toP1, uint256 toP2, uint256 gain) {
        uint256 two = stake * 2;
        if (kind == CertLib.HOMOG_BOTH_CORRECT) return (stake, stake, 0);
        if (kind == CertLib.HOMOG_P1_CORRECT) return (stake / 2, 0, two - stake / 2);
        if (kind == CertLib.HOMOG_P2_CORRECT) return (0, stake / 2, two - stake / 2);
        if (kind == CertLib.BOTH_WRONG || kind == CertLib.TIMEOUT_BOTH_FORFEIT) return (0, 0, two);
        if (kind == CertLib.HETERO_P1_WINS || kind == CertLib.TIMEOUT_P1_WINS) return (two, 0, 0);
        if (kind == CertLib.HETERO_P2_WINS || kind == CertLib.TIMEOUT_P2_WINS) return (0, two, 0);
        revert BadOutcome();
    }

    function _treasuryCut(uint256 amount) private view returns (uint256 cut, uint256 remainder) {
        cut = amount * treasurySplitBps / BPS_DENOM;
        remainder = amount - cut;
    }

    function _pay(address to, uint256 amount) private {
        (bool ok,) = payable(to).call{value: amount}("");
        require(ok, "transfer failed");
    }

    // ---------------------------------------------------------------------
    // Owner operations
    // ---------------------------------------------------------------------

    function withdrawPrizePool(address to, uint256 amount) external onlyOwner nonReentrant {
        if (to == address(0)) revert BadConfig();
        if (amount > prizePoolWei) revert BadStake();
        prizePoolWei -= amount;
        _pay(to, amount);
        emit PrizePoolWithdrawn(to, amount);
    }

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }
}
