// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {CertLib} from "./CertLib.sol";
import {PullPayment} from "./base/PullPayment.sol";
import {AttesterGated} from "./base/AttesterGated.sol";

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
///   Pending    ─cancelPending──► Cancelled [p1 refunded, no opponent ever joined]
///   Committing ─commitGuess(second)─► Revealing
///   Committing ─resolveTimeout──► Resolved [committer wins]
///   Revealing  ─revealGuess(both)──► Resolved [payoff matrix]
///   Revealing  ─resolveTimeout──► Resolved [revealer wins / both forfeit]
///   Resolved   ─(terminal)
///   Cancelled  ─(terminal)
contract CoordinationGame is Ownable2Step, PullPayment, Pausable, AttesterGated {
    enum Status {
        None,
        Pending,
        Active,
        Committing,
        Revealing,
        Resolved,
        Cancelled
    }

    struct Game {
        Status status;
        address player1; // creator
        address player2; // joiner
        uint8 p1Guess; // 255 = unrevealed
        uint8 p2Guess; // 255 = unrevealed
        uint8 firstCommitter; // 1 | 2; 0 = unset (Solana/CertLib checkpoints use 255)
        uint8 matchupType; // 0 | 1; 255 = unset (revealed by the first revealer)
        uint128 stakeWei;
        bytes32 matchupCommitment;
        bytes32 p1Commit; // 0 = not committed
        bytes32 p2Commit;
        uint64 activatedAt; // join time — Active/commit-timeout anchor
        uint64 firstCommitAt; // first commit time — Committing-timeout anchor
        uint64 bothCommitAt; // second commit time — Revealing-timeout anchor
        uint64 createdAt; // create time — Pending join-window / cancel anchor (H3)
        uint32 commitWindowSecs; // timeout windows SNAPSHOTTED at create (M3) so a
        uint32 revealWindowSecs; // later setConfig can't force-resolve this game early
    }

    mapping(bytes32 => Game) public games;

    /// A wallet-authorized ephemeral session key — the EVM port of the Solana
    /// `SessionAuthority` PDA. The wallet stays the on-chain player and payout
    /// recipient (stake never leaves its identity, so a lost session key can
    /// never strand or capture funds); the session key only signs the non-payable
    /// commit/reveal/timeout steps, so it holds gas dust only.
    struct SessionAuth {
        address sessionKey;
        uint64 expiry;
    }

    /// player wallet => its active session authorization.
    mapping(address => SessionAuth) public sessions;
    /// player wallet => auth nonce; bumped on every authorize/revoke so an old
    /// authorization signature can never be replayed after it is superseded.
    mapping(address => uint256) public sessionNonce;

    // Pull-payment ledger (M1) — `withdrawable` / `_credit` / `withdraw()`
    // live in the shared PullPayment base.

    /// Accumulates homogeneous-outcome forfeits — the local analog of
    /// Tournament.prize_lamports; the owner sweeps it to the tournament pot.
    uint256 public prizePoolWei;

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

    // Config sanity bounds (L1).
    uint128 private constant MIN_STAKE_WEI = 0.0001 ether;
    uint128 private constant MAX_STAKE_WEI = 100 ether;
    uint32 private constant MIN_WINDOW_SECS = 60;
    uint32 private constant MAX_WINDOW_SECS = 30 days;

    event GameCreated(bytes32 indexed gameId, address indexed player1, uint128 stakeWei);
    event GameStarted(bytes32 indexed gameId, address indexed player1, address indexed player2);
    event GuessCommitted(bytes32 indexed gameId, address indexed player);
    event GuessRevealed(bytes32 indexed gameId, address indexed player);
    event GameResolved(bytes32 indexed gameId, uint8 outcomeKind, uint256 toP1, uint256 toP2, uint256 toTreasury);
    event PrizePoolWithdrawn(address indexed to, uint256 amount);
    event ConfigUpdated();
    event GameCancelled(bytes32 indexed gameId, address indexed player1, uint256 amount);
    event SessionAuthorized(address indexed player, address indexed sessionKey, uint64 expiry);
    event SessionRevoked(address indexed player);

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
    error BadSession();

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
        _setAuthorizedSigner(operatorSigner_);
        treasury = treasury_;
        treasurySplitBps = treasurySplitBps_;
        stakeWei = stakeWei_;
        commitTimeoutSecs = commitTimeoutSecs_;
        revealTimeoutSecs = revealTimeoutSecs_;
    }

    /// @notice Dedicated secp256k1 matchmaker key — NOT the owner and not the
    ///         treasury (key separation: matchmaker compromise can't move
    ///         governance/treasury). Stored in the AttesterGated base.
    function operatorSigner() public view returns (address) {
        return _authorizedSignerAddr();
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
        if (operatorSigner_ == address(0) || treasury_ == address(0)) revert BadConfig();
        // Key separation: the matchmaker key is never the owner or the treasury.
        if (operatorSigner_ == owner_ || operatorSigner_ == treasury_) revert BadConfig();
        if (treasurySplitBps_ < MIN_SPLIT_BPS || treasurySplitBps_ > MAX_SPLIT_BPS) revert BadConfig();
        // Bounds (L1): stake + the two timeout windows.
        if (stakeWei_ < MIN_STAKE_WEI || stakeWei_ > MAX_STAKE_WEI) revert BadConfig();
        if (commitTimeoutSecs_ < MIN_WINDOW_SECS || commitTimeoutSecs_ > MAX_WINDOW_SECS) revert BadConfig();
        if (revealTimeoutSecs_ < MIN_WINDOW_SECS || revealTimeoutSecs_ > MAX_WINDOW_SECS) revert BadConfig();
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
        _setAuthorizedSigner(operatorSigner_);
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
    /// @notice Stake and open a game AS `player`. `msg.sender` must act for
    ///         `player` — either `player` itself (direct: an agent/self-stake) or
    ///         `player`'s authorized session key (wallet-as-player: the wallet
    ///         pre-funded the session via {openSession}). The recorded on-chain
    ///         creator is always `player` (the wallet), never the session signer,
    ///         so identity, escrow, and payout all bind to the wallet natively.
    function createGame(bytes32 gameId, bytes32 matchupCommitment, bytes calldata operatorSig, address player)
        external
        payable
        whenNotPaused
    {
        Game storage g = games[gameId];
        if (g.status != Status.None) revert InvalidStatus();
        if (msg.value != stakeWei) revert BadStake();
        if (matchupCommitment == bytes32(0)) revert BadCommitment();
        if (!_actsFor(msg.sender, player)) revert BadSession();
        // Bind `player` (the recorded creator) into the digest (L2): the operator
        // attests THIS wallet, so a sig signed for one player can't be replayed by
        // another to squat the assigned gameId.
        bytes32 digest = keccak256(abi.encode(block.chainid, address(this), gameId, player, matchupCommitment));
        if (!_recoverAndCheck(digest, operatorSig)) revert BadSignature();

        g.status = Status.Pending;
        g.player1 = player;
        g.stakeWei = uint128(msg.value);
        g.matchupCommitment = matchupCommitment;
        g.p1Guess = UNREVEALED;
        g.p2Guess = UNREVEALED;
        g.firstCommitter = 0;
        g.matchupType = MATCHUP_UNSET;
        g.createdAt = uint64(block.timestamp);
        // Freeze this game's timeout schedule at the config in force now (M3).
        g.commitWindowSecs = commitTimeoutSecs;
        g.revealWindowSecs = revealTimeoutSecs;
        emit GameCreated(gameId, player, g.stakeWei);
    }

    /// @notice Refund the creator of a Pending game that no one ever joined, once
    ///         the join window (commitTimeoutSecs after creation) elapses.
    ///         Permissionless — anyone may crank it; the stake is credited back to
    ///         player1 (M1 pull-payment). Without this an unjoined game stranded
    ///         P1's stake forever (audit H3). A P2 that joins within the window
    ///         moves the game to Active, after which this reverts (not Pending).
    function cancelPending(bytes32 gameId) external {
        Game storage g = games[gameId];
        if (g.status != Status.Pending) revert InvalidStatus();
        if (block.timestamp <= uint256(g.createdAt) + g.commitWindowSecs) revert DeadlineNotReached();

        g.status = Status.Cancelled;
        uint256 amount = g.stakeWei;
        emit GameCancelled(gameId, g.player1, amount);
        _credit(g.player1, amount);
    }

    /// @notice Join a pending game AS `player` by matching the stake; goes Active.
    ///         `msg.sender` must act for `player` (direct or via session key). The
    ///         recorded player2 is `player` (the wallet), not the session signer.
    function joinGame(bytes32 gameId, address player) external payable whenNotPaused {
        Game storage g = games[gameId];
        if (g.status != Status.Pending) revert InvalidStatus();
        if (!_actsFor(msg.sender, player)) revert BadSession();
        if (player == g.player1) revert NotParticipant();
        if (msg.value != g.stakeWei) revert BadStake();

        g.player2 = player;
        g.status = Status.Active;
        g.activatedAt = uint64(block.timestamp);
        emit GameStarted(gameId, g.player1, player);
    }

    // ---------------------------------------------------------------------
    // Session authorization (wallet-as-player delegation) — the EVM port of the
    // Solana SessionAuthority. The wallet signs ONE domain-bound authorization
    // off-chain; the session key (which pays gas) submits it. The wallet remains
    // the on-chain player and payout recipient, so commit/reveal/timeout can be
    // driven by the gas-only session key with no stake movement or sweep-back.
    // ---------------------------------------------------------------------

    /// @notice The digest a wallet signs (EIP-191 personal_sign) to authorize
    ///         `sessionKey` until `expiry`. Domain-bound to this chain+contract
    ///         and nonce-bound so it can't be replayed after a later authorize or
    ///         revoke bumps the nonce.
    function sessionAuthDigest(address player, address sessionKey, uint64 expiry) public view returns (bytes32) {
        return keccak256(
            abi.encode(block.chainid, address(this), "COORD_SESSION", player, sessionKey, expiry, sessionNonce[player])
        );
    }

    /// @notice Authorize `sessionKey` to act for the signing wallet in
    ///         commit/reveal/timeout. The wallet signs `sessionAuthDigest`
    ///         off-chain; anyone (typically the session key) submits it.
    function authorizeSession(address player, address sessionKey, uint64 expiry, bytes calldata playerSig) external {
        if (sessionKey == address(0) || expiry <= block.timestamp) revert BadSession();
        bytes32 digest = MessageHashUtils.toEthSignedMessageHash(sessionAuthDigest(player, sessionKey, expiry));
        if (ECDSA.recover(digest, playerSig) != player) revert BadSignature();
        sessions[player] = SessionAuth(sessionKey, expiry);
        unchecked {
            sessionNonce[player]++;
        }
        emit SessionAuthorized(player, sessionKey, expiry);
    }

    /// @notice Revoke the caller's session immediately (and invalidate any
    ///         pending authorization signatures by bumping the nonce).
    function revokeSession() external {
        delete sessions[msg.sender];
        unchecked {
            sessionNonce[msg.sender]++;
        }
        emit SessionRevoked(msg.sender);
    }

    /// @notice Register `sessionKey` as the caller's delegate until `expiry` AND
    ///         fund it with `msg.value` (gas + a stake buffer) in a single tx —
    ///         the ONE wallet popup that opens a session. Afterwards the gas-only
    ///         session key drives createGame/joinGame/commit/reveal on the wallet's
    ///         behalf with zero further popups, while the wallet stays the on-chain
    ///         player and payout recipient. This is the wallet-called counterpart
    ///         to {authorizeSession} (which takes an off-chain sig instead): here
    ///         `msg.sender` IS the authorizing player, so no signature is needed.
    ///         CEI + nonReentrant — session state is written before the ETH is
    ///         forwarded to the session EOA.
    function openSession(address sessionKey, uint64 expiry) external payable nonReentrant {
        if (sessionKey == address(0) || expiry <= block.timestamp) revert BadSession();
        sessions[msg.sender] = SessionAuth(sessionKey, expiry);
        unchecked {
            sessionNonce[msg.sender]++;
        }
        emit SessionAuthorized(msg.sender, sessionKey, expiry);
        if (msg.value > 0) {
            (bool ok,) = payable(sessionKey).call{value: msg.value}("");
            require(ok, "gas fund failed");
        }
    }

    /// @dev True iff `actor` may act for `player` — the wallet itself, or its
    ///      currently-authorized, unexpired session key.
    function _actsFor(address actor, address player) private view returns (bool) {
        if (actor == player) return true;
        SessionAuth storage s = sessions[player];
        return s.sessionKey == actor && block.timestamp < s.expiry;
    }

    /// @dev Resolve which player `msg.sender` acts for (directly or via session).
    ///      Reverts NotParticipant if it acts for neither. Returns isP1.
    function _actingSeat(Game storage g) private view returns (bool isP1) {
        if (_actsFor(msg.sender, g.player1)) return true;
        if (_actsFor(msg.sender, g.player2)) return false;
        revert NotParticipant();
    }

    /// @notice Commit SHA-256(r) of a hidden guess preimage. First commit moves
    ///         the game to Committing and records the first committer (the
    ///         heterogeneous-tie-break seat); second commit moves it to Revealing.
    function commitGuess(bytes32 gameId, bytes32 commitment) external {
        Game storage g = games[gameId];
        if (g.status != Status.Active && g.status != Status.Committing) revert InvalidStatus();
        if (commitment == bytes32(0)) revert BadCommitment();
        // The wallet OR its authorized session key may act; the action is
        // attributed to the wallet-player (isP1), never to msg.sender.
        bool isP1 = _actingSeat(g);

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
        // The wallet OR its authorized session key may reveal; attributed to the
        // wallet-player, so the payout still lands on the wallet.
        bool isP1 = _actingSeat(g);

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
            _requireElapsed(g.activatedAt, g.commitWindowSecs);
            stepCount = 0;
        } else if (g.status == Status.Committing) {
            _requireElapsed(g.firstCommitAt, g.commitWindowSecs);
            stepCount = 1;
        } else if (g.status == Status.Revealing) {
            _requireElapsed(g.bothCommitAt, g.revealWindowSecs);
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

        // Interactions — credit, never push (M1): a reverting player or treasury
        // must not block the game from resolving.
        if (toTreasury > 0) _credit(treasury, toTreasury);
        if (toP1 > 0) _credit(g.player1, toP1);
        if (toP2 > 0) _credit(g.player2, toP2);
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

    // Pull-payment credit + withdraw() live in the shared PullPayment base.

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
