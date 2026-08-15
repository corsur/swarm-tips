// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {CertLib} from "./CertLib.sol";
import {PullPayment} from "./base/PullPayment.sol";
import {SeasonPot} from "./SeasonPot.sol";
import {Initializable} from "../lib/openzeppelin-contracts/contracts/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "../lib/openzeppelin-contracts/contracts/proxy/utils/UUPSUpgradeable.sol";
import {AttesterGated} from "./base/AttesterGated.sol";

/// @title CoordinationGameV4 - the game, plus the tournament EVM never had
///
/// @notice v4 = v3's proven lifecycle, unchanged, PLUS:
///           - a SeasonPot: 1-year seasons, per-season player records, and a
///             permissionless merkle claim, mirroring Solana's tournament
///           - a UUPS proxy, so a fix is a patch instead of a redeploy
///
///         v3 is immutable BY DEFAULT, not by decision: there is no rationale
///         in any header or in multichain/decision.md. It is the Foundry
///         default, never revisited, while the Solana program has been
///         upgradeable all along. Every fix to v1/v3 was therefore a redeploy
///         with state stranded at the old address.
///
/// @dev Deployed BEHIND AN ERC1967 PROXY. The implementation constructor only
///      disables initializers; all configuration happens in `initialize`.
///
/// @title CoordinationGameInheritedDoc — same-chain (EVM-vs-EVM) 1v1 commit-reveal game
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
contract CoordinationGameV4 is
    Initializable,
    UUPSUpgradeable,
    Ownable2Step,
    PullPayment,
    Pausable,
    AttesterGated,
    SeasonPot
{
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

    address public treasury;
    uint16 public treasurySplitBps; // [2000, 8000], mirrors GlobalConfig bounds
    uint128 public stakeWei; // exact required stake
    uint32 public commitTimeoutSecs; // Active + Committing stages
    uint32 public revealTimeoutSecs; // Revealing stage

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
    event ConfigUpdated();
    event GameCancelled(bytes32 indexed gameId, address indexed player1, uint256 amount);
    event SessionAuthorized(address indexed player, address indexed sessionKey, uint64 expiry);
    event SessionRevoked(address indexed player);

    event Deposited(address indexed player, uint256 amount);

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

    /// @dev Implementation constructor. Locks the implementation so it can
    ///      never be initialized directly: an uninitialized implementation is
    ///      one of the classic UUPS takeover routes.
    /// @dev `Ownable(msg.sender)` writes to the IMPLEMENTATION's storage, not
    ///      the proxy's, and the implementation is locked immediately after.
    ///      The proxy's real owner is set by `initialize`.
    constructor() Ownable(msg.sender) {
        _disableInitializers();
    }

    /// @notice Proxy initializer, replacing v3's constructor. A proxy's state
    ///         lives in the PROXY, so constructor assignment would write to the
    ///         wrong storage entirely.
    function initialize(
        address initialOwner,
        address operatorSigner_,
        address treasury_,
        uint16 treasurySplitBps_,
        uint128 stakeWei_,
        uint32 commitTimeoutSecs_,
        uint32 revealTimeoutSecs_,
        uint256 firstSeasonId,
        uint64 firstSeasonDurationSecs
    ) external initializer {
        _validateConfig(
            initialOwner,
            operatorSigner_,
            treasury_,
            treasurySplitBps_,
            stakeWei_,
            commitTimeoutSecs_,
            revealTimeoutSecs_
        );
        _transferOwnership(initialOwner);
        _setAuthorizedSigner(operatorSigner_);
        treasury = treasury_;
        treasurySplitBps = treasurySplitBps_;
        stakeWei = stakeWei_;
        commitTimeoutSecs = commitTimeoutSecs_;
        revealTimeoutSecs = revealTimeoutSecs_;
        _startSeason(firstSeasonId, firstSeasonDurationSecs);
    }

    /// @dev Only the owner may upgrade. The single most dangerous function on
    ///      the contract: it can replace all logic and so reach every staked wei.
    function _authorizeUpgrade(address) internal override onlyOwner {}

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
        if (matchupCommitment == bytes32(0)) revert BadCommitment();
        if (!_actsFor(msg.sender, player)) revert BadSession();
        _takeStake(player, stakeWei);
        // Bind `player` (the recorded creator) into the digest (L2): the operator
        // attests THIS wallet, so a sig signed for one player can't be replayed by
        // another to squat the assigned gameId.
        bytes32 digest = keccak256(abi.encode(block.chainid, address(this), gameId, player, matchupCommitment));
        if (!_recoverAndCheck(digest, operatorSig)) revert BadSignature();

        g.status = Status.Pending;
        g.player1 = player;
        g.stakeWei = uint128(stakeWei);
        g.matchupCommitment = matchupCommitment;
        g.p1Guess = CertLib.UNREVEALED;
        g.p2Guess = CertLib.UNREVEALED;
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
    function joinGame(bytes32 gameId, address player, bytes calldata operatorSig)
        external
        payable
        whenNotPaused
    {
        Game storage g = games[gameId];
        if (g.status != Status.Pending) revert InvalidStatus();
        if (!_actsFor(msg.sender, player)) revert BadSession();
        if (player == g.player1) revert NotParticipant();
        // Bind `player` (the joiner) into an operator-signed digest, mirroring
        // createGame's L2 binding and the Solana matchmaker cosign: the operator
        // attests THIS wallet is the paired opponent, so joinGame is no longer a
        // permissionless open slot a stranger could front-run while Pending.
        bytes32 digest = keccak256(abi.encode(block.chainid, address(this), gameId, player));
        if (!_recoverAndCheck(digest, operatorSig)) revert BadSignature();
        _takeStake(player, g.stakeWei);

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
    ///         fund it with `msg.value` in a single tx — the ONE wallet popup that
    ///         opens a session. Afterwards the gas-only session key drives
    ///         createGame/joinGame/commit/reveal on the wallet's behalf with zero
    ///         further popups, while the wallet stays the on-chain player and
    ///         payout recipient. This is the wallet-called counterpart to
    ///         {authorizeSession} (which takes an off-chain sig instead): here
    ///         `msg.sender` IS the authorizing player, so no signature is needed.
    ///         CEI + nonReentrant — session state is written before the ETH is
    ///         forwarded to the session EOA.
    ///
    /// @dev    SEND GAS ONLY. This doc previously read "gas + a stake buffer",
    ///         which described the pre-escrow model: the stake rode along to the
    ///         session EOA and sat there until createGame spent it, so a lost or
    ///         expired session key took it with it, unrecoverably. That is the
    ///         exact custody gap {deposit} closed — stake the escrow balance
    ///         instead and leave only gas here, matching Solana, where the session
    ///         key holds gas and `StakeEscrow` (a program-owned PDA) holds the
    ///         stake. Sending the stake here still works — createGame/joinGame are
    ///         dual-mode — but it forfeits the recoverability.
    function openSession(address sessionKey, uint64 expiry) external payable nonReentrant {
        _registerSession(sessionKey, expiry);
        _fundSession(sessionKey, msg.value);
    }

    /// @notice Open a session AND escrow the stake in ONE transaction: forwards
    ///         `gasAmount` to the session EOA and credits the remainder to the
    ///         caller's {withdrawable} balance, which {createGame}/{joinGame}
    ///         then debit with `msg.value == 0`.
    ///
    /// @dev    WHY THIS EXISTS. {deposit} credits `msg.sender` and `_takeStake`
    ///         debits `withdrawable[player]` — the WALLET — so an escrowed stake
    ///         cannot be sent by the session key. Without this the escrow flow
    ///         would cost a second wallet popup on every first game, where the
    ///         whole game costs exactly one today. Combining them keeps that
    ///         property AND gets the stake out of the ephemeral key.
    ///
    ///         CEI + nonReentrant: session state and the credit are both written
    ///         before any ETH leaves. The credit is deliberately made BEFORE the
    ///         forward, so a hostile session key that reverts cannot strand a
    ///         player mid-way with their stake neither in the key nor the ledger.
    function openSessionAndDeposit(address sessionKey, uint64 expiry, uint256 gasAmount) external payable nonReentrant {
        // Checks
        if (gasAmount > msg.value) revert BadStake();

        // Effects
        _registerSession(sessionKey, expiry);
        uint256 staked = msg.value - gasAmount; // checked above
        if (staked > 0) {
            _credit(msg.sender, staked);
            emit Deposited(msg.sender, staked);
        }

        // Interactions
        _fundSession(sessionKey, gasAmount);
    }

    /// @dev Record `sessionKey` as `msg.sender`'s delegate. Shared by both entry
    ///      points so the authorization, nonce bump and event have ONE definition.
    function _registerSession(address sessionKey, uint64 expiry) private {
        if (sessionKey == address(0) || expiry <= block.timestamp) revert BadSession();
        sessions[msg.sender] = SessionAuth(sessionKey, expiry);
        unchecked {
            sessionNonce[msg.sender]++;
        }
        emit SessionAuthorized(msg.sender, sessionKey, expiry);
    }

    /// @dev Forward gas to the session EOA. No-op at zero so a session may be
    ///      opened without funding (or re-opened when the key is already funded).
    function _fundSession(address sessionKey, uint256 amount) private {
        if (amount == 0) return;
        (bool ok,) = payable(sessionKey).call{value: amount}("");
        require(ok, "gas fund failed");
    }

    // ---------------------------------------------------------------------
    // Escrow parity with Solana (v5)
    //
    // Before this, the stake rested in the SESSION KEY between openSession and
    // createGame. Solana never did that: `deposit_stake` moves the stake into a
    // program-owned `StakeEscrow` PDA, and `withdraw_stake` returns it to the
    // player. The difference is recoverability, not convenience — ETH sitting
    // in a lost or expired session key is gone, whereas the PDA is always
    // reclaimable by the player's own signature.
    //
    // `withdrawable` IS the escrow. Reusing the existing PullPayment ledger
    // rather than adding a mapping is what keeps this a logic-only upgrade:
    // no storage slot moves, so CoordinationGameV4Layout.t.sol stays green and
    // a live contract holding real funds is upgraded without touching state.
    // It also means winnings and staged stake share one balance, so a second
    // game costs no new deposit.
    // ---------------------------------------------------------------------

    /// @notice Pre-fund your escrow balance. The same ledger settlement credits,
    ///         so a player may also just leave winnings in place and re-stake.
    function deposit() external payable {
        if (msg.value == 0) revert BadStake();
        _credit(msg.sender, msg.value);
        emit Deposited(msg.sender, msg.value);
    }

    /// @dev Take `amount` of stake for `player`, from `msg.value` or from the
    ///      escrow balance. DUAL-MODE deliberately: this is a live contract, and
    ///      a client that still sends the stake inline must keep working across
    ///      the upgrade, which also lets the frontend migrate on its own clock.
    ///
    ///      A partial `msg.value` is REJECTED rather than topped up from the
    ///      balance. Mixing the two sources would make the amount actually taken
    ///      depend on a balance the caller may not have checked — reject at the
    ///      boundary instead of guessing.
    function _takeStake(address player, uint256 amount) private {
        if (msg.value == amount) return;
        if (msg.value != 0) revert BadStake();
        uint256 bal = withdrawable[player];
        if (bal < amount) revert BadStake();
        // Storage-only debit; no external call, so no reentrancy surface and no
        // CEI ordering concern. Checked arithmetic — `bal >= amount` above.
        withdrawable[player] = bal - amount;
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
            if (g.p1Guess != CertLib.UNREVEALED) revert AlreadyActed();
            g.p1Guess = guess;
        } else {
            if (g.p2Guess != CertLib.UNREVEALED) revert AlreadyActed();
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

        if (g.p1Guess != CertLib.UNREVEALED && g.p2Guess != CertLib.UNREVEALED) {
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
        // v3 put this in a single owner-drainable `prizePoolWei` with NO claim
        // path -- the exact defect v4 exists to fix. Forfeits now accrue to the
        // OPEN SEASON and are claimable by the players who earned them.
        _accrue(toPrize);

        // Interactions — credit, never push (M1): a reverting player or treasury
        // must not block the game from resolving.
        if (toTreasury > 0) _credit(treasury, toTreasury);
        if (toP1 > 0) _credit(g.player1, toP1);
        if (toP2 > 0) _credit(g.player2, toP2);
        // Per-season record. p1Won/p2Won come from the SHARED core's
        // outcome_to_wins and are NOT derivable from the amounts above:
        // HOMOG_BOTH_CORRECT returns each player their own stake (zero net
        // gain) and yet awards BOTH a win, because a win records a correct
        // read of the opponent rather than a profit.
        (bool p1Won, bool p2Won) = _winsFor(outcomeKind);
        _recordResult(g.player1, g.player2, p1Won, p2Won);

        emit GameResolved(gameId, outcomeKind, toP1, toP2, toTreasury);
    }

    /// @dev The canonical payoff matrix as pot amounts — the EVM mirror of
    ///      payoff.rs::resolve_game, keyed by CertLib's outcome kind so it can
    ///      never disagree with the derivation.
    function _amounts(uint8 kind, uint256 stake) internal pure returns (uint256 toP1, uint256 toP2, uint256 gain) {
        uint256 two = stake * 2;
        if (kind == CertLib.HOMOG_BOTH_CORRECT) return (stake, stake, 0);
        if (kind == CertLib.HOMOG_P1_CORRECT) return (stake / 2, 0, two - stake / 2);
        if (kind == CertLib.HOMOG_P2_CORRECT) return (0, stake / 2, two - stake / 2);
        if (kind == CertLib.BOTH_WRONG || kind == CertLib.TIMEOUT_BOTH_FORFEIT) return (0, 0, two);
        if (kind == CertLib.HETERO_P1_WINS || kind == CertLib.TIMEOUT_P1_WINS) return (two, 0, 0);
        if (kind == CertLib.HETERO_P2_WINS || kind == CertLib.TIMEOUT_P2_WINS) return (0, two, 0);
        revert BadOutcome();
    }

    /// @dev Mirror of `chain_core::game::outcome_to_wins`, pinned by
    ///      tests/fixtures/game-payout-vectors.json (p1Won / p2Won columns).
    function _winsFor(uint8 kind) internal pure returns (bool p1Won, bool p2Won) {
        if (kind == CertLib.HOMOG_BOTH_CORRECT) return (true, true);
        if (kind == CertLib.HOMOG_P1_CORRECT || kind == CertLib.HETERO_P1_WINS || kind == CertLib.TIMEOUT_P1_WINS) {
            return (true, false);
        }
        if (kind == CertLib.HOMOG_P2_CORRECT || kind == CertLib.HETERO_P2_WINS || kind == CertLib.TIMEOUT_P2_WINS) {
            return (false, true);
        }
        return (false, false); // BOTH_WRONG, TIMEOUT_BOTH_FORFEIT
    }

    // ---------------------------------------------------------------------
    // Season entry points
    // ---------------------------------------------------------------------

    /// @notice Open the next season. Callable while the current one is still
    ///         running: the rollover hazard is having no NEXT season ready,
    ///         which is exactly what took Solana mainnet down on 2026-08-06.
    function startSeason(uint256 seasonId, uint64 durationSecs) external onlyOwner {
        _startSeason(seasonId, durationSecs);
    }

    /// @notice Publish a season's merkle root once it has EXPIRED.
    ///         A season may only promise what IT accrued, so it can never
    ///         promise another season's money.
    function finalizeSeason(uint256 seasonId, bytes32 root, uint256 totalWei) external onlyOwner {
        _finalizeSeason(seasonId, root, totalWei);
    }

    /// @notice Claim a season prize. PERMISSIONLESS, like Solana's
    ///         `claim_reward` - the whole point is that the pot is not the
    ///         owner's to hand out.
    /// @dev DELIBERATELY NOT `whenNotPaused`. Pausing stops NEW games from
    ///      being created or joined; it must never be able to withhold money a
    ///      player has already earned. A pausable claim would make the pot the
    ///      owner's to release after all, which is the exact property v4 exists
    ///      to remove.
    function claimPrize(uint256 seasonId, uint256 amount, bytes32[] calldata proof) external nonReentrant {
        uint256 owed = _claim(seasonId, msg.sender, amount, proof);
        _credit(msg.sender, owed); // pull payment: a reverting claimant cannot brick anyone else
    }

    /// @notice Sweep what nobody claimed, after the grace window.
    function sweepUnclaimed(uint256 seasonId, address to) external onlyOwner nonReentrant {
        uint256 amount = _sweepUnclaimed(seasonId);
        _credit(to, amount);
        emit UnclaimedSwept(seasonId, to, amount);
    }

    function _treasuryCut(uint256 amount) private view returns (uint256 cut, uint256 remainder) {
        cut = amount * treasurySplitBps / BPS_DENOM;
        remainder = amount - cut;
    }

    // Pull-payment credit + withdraw() live in the shared PullPayment base.

    // ---------------------------------------------------------------------
    // Owner operations
    // ---------------------------------------------------------------------

    // withdrawPrizePool is DELETED. It let the owner drain every forfeit while
    // players had no claim path at all. Its replacement is sweepUnclaimed,
    // which can only take what nobody claimed and only after the grace window.

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }
}
