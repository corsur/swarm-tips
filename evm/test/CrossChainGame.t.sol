// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CrossChainGame} from "../src/CrossChainGame.sol";
import {CertLib} from "../src/CertLib.sol";
import {PullPayment} from "../src/base/PullPayment.sol";

/// @notice Behavioral suite for the EVM leg: state machine, the panel's
///         eight flagged gaps (double-claim, inert cert, stale quote,
///         equivocation, refund-vs-skew timing, tranche clamp), and payoff
///         conservation (fuzzed). Pool accounting lives in the sibling
///         CrossChainGameInvariant.t.sol — this file declares no invariant_*
///         test, so claiming one here sent the reader looking for it.
contract CrossChainGameTest is Test {
    CrossChainGame internal game;

    bytes32 internal constant CHAIN_TAG = keccak256("eip155:84532");
    uint128 internal constant STAKE = 0.0025 ether;
    uint128 internal constant MAX_TRANCHE = 0.005 ether;
    // M6: generous so ordinary tests never trip the daily cap; the dedicated M6
    // test lowers it via setConfig to prove the rolling-24h cap enforces.
    uint128 internal constant DAILY_CAP = 100 ether;
    uint32 internal constant CLAIM_WINDOW = 3600;
    uint32 internal constant SKEW = 900;
    uint16 internal constant SPLIT_BPS = 5000;
    // Matchup reveal preimage with LSB == 1 (matches the checkpoints' matchupType
    // of 1); the cert's matchupCommitment is its sha256 so the terminal-checkpoint
    // binding (finding #5) holds.
    bytes32 internal constant R_MATCHUP = bytes32(uint256(0xE1));

    address internal owner = makeAddr("owner");
    address internal treasury = makeAddr("treasury");
    address internal player = makeAddr("player");

    uint256 internal operatorPk = 0xA11CE;
    address internal operatorSigner;
    uint256 internal localSessionPk = 0xB0B;
    address internal localSession;
    uint256 internal counterSessionPk = 0xCa7;
    address internal counterSession;

    function setUp() public {
        // Realistic 2026 unix time so quote/deadline math never underflows
        // against Foundry's default block.timestamp of 1.
        vm.warp(1_765_000_000);
        operatorSigner = vm.addr(operatorPk);
        localSession = vm.addr(localSessionPk);
        counterSession = vm.addr(counterSessionPk);

        vm.prank(owner);
        game = new CrossChainGame(
            CHAIN_TAG, owner, operatorSigner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, DAILY_CAP, CLAIM_WINDOW, SKEW
        );

        vm.deal(owner, 100 ether);
        vm.deal(player, 10 ether);
        vm.prank(owner);
        game.poolDeposit{value: 10 ether}();
    }

    // --- helpers ---------------------------------------------------------

    function _fund(bytes32 matchId, bool playerIsP1) internal {
        uint64 fundDeadline = uint64(block.timestamp + 1 days);
        uint64 matchDeadline = uint64(block.timestamp + 2 days);
        vm.prank(player);
        game.createMatch{value: STAKE}(
            matchId,
            localSession,
            counterSession,
            playerIsP1,
            fundDeadline,
            matchDeadline,
            _createMatchSig(matchId, player, localSession, counterSession, playerIsP1, fundDeadline, matchDeadline)
        );
    }

    /// Operator authorization for createMatch (M4) — the digest the contract checks.
    function _createMatchSig(
        bytes32 matchId,
        address player_,
        address sessionKey,
        address counterSessionKey,
        bool playerIsP1,
        uint64 fundDeadline,
        uint64 matchDeadline
    ) internal view returns (bytes memory) {
        bytes32 digest = keccak256(
            abi.encode(
                block.chainid,
                address(game),
                matchId,
                player_,
                sessionKey,
                counterSessionKey,
                playerIsP1,
                fundDeadline,
                matchDeadline
            )
        );
        return _sign(operatorPk, digest);
    }

    function _playerIsP1(bytes32 matchId) internal view returns (bool p) {
        (,,,, p,,,,,,,,,,,,) = game.matches(matchId);
    }

    /// Permissionless lock: build a cert matching the funded match and the
    /// operator's match-live signature (the same sig relayed at pairing). No
    /// `owner` prank — anyone may submit + pay gas.
    function _lock(bytes32 matchId, uint128 tranche) internal {
        CertLib.MatchLiveCert memory cert = _cert(matchId, _playerIsP1(matchId), tranche);
        game.lockTranche(cert, _sign(operatorPk, CertLib.matchLiveDigest(cert)));
    }

    /// Build a match-live cert whose EVM leg (legB) matches the recorded
    /// escrow terms for `matchId`. Uses the default fund-time matchDeadline.
    function _cert(bytes32 matchId, bool playerIsP1, uint128 tranche)
        internal
        view
        returns (CertLib.MatchLiveCert memory cert)
    {
        return _certMd(matchId, playerIsP1, tranche, uint64(block.timestamp), uint64(block.timestamp + 2 days));
    }

    /// Same as `_cert` but with explicit `quoteTimestamp` + `matchDeadline`, so a
    /// lock built after a time-warp still matches a match funded earlier (the
    /// contract binds cert.matchDeadline == m.matchDeadline and checks quote
    /// freshness against the warped block time — and `vm.warp` does not update
    /// the test frame's own `block.timestamp` reads, so both must be passed in).
    function _certMd(bytes32 matchId, bool playerIsP1, uint128 tranche, uint64 quoteTimestamp, uint64 matchDeadline)
        internal
        view
        returns (CertLib.MatchLiveCert memory cert)
    {
        CertLib.Leg memory legA = CertLib.Leg({
            chainTag: keccak256("solana:devnet"),
            contractId: keccak256("solana-program"),
            player: keccak256("solana-player"),
            sessionKey: counterSession,
            stake: 0.05 ether,
            tranche: tranche
        });
        CertLib.Leg memory legB = CertLib.Leg({
            chainTag: CHAIN_TAG,
            contractId: bytes32(uint256(uint160(address(game)))),
            player: bytes32(uint256(uint160(player))),
            sessionKey: localSession,
            stake: STAKE,
            tranche: tranche
        });
        cert = CertLib.MatchLiveCert({
            matchId: matchId,
            tournamentId: 1,
            matchupCommitment: sha256(abi.encodePacked(R_MATCHUP)),
            legA: legA,
            legB: legB,
            quoteTimestamp: quoteTimestamp,
            quoteMaxAgeSecs: 300,
            matchDeadline: matchDeadline,
            claimWindowSecs: CLAIM_WINDOW,
            aIsP1: playerIsP1 ? 0 : 1
        });
    }

    /// Fund a match with explicit deadlines (default `_fund` derives them from
    /// fund-time, which a later warp would invalidate).
    function _fundWithDeadlines(bytes32 matchId, bool playerIsP1, uint64 fundDeadline, uint64 matchDeadline) internal {
        vm.prank(player);
        game.createMatch{value: STAKE}(
            matchId,
            localSession,
            counterSession,
            playerIsP1,
            fundDeadline,
            matchDeadline,
            _createMatchSig(matchId, player, localSession, counterSession, playerIsP1, fundDeadline, matchDeadline)
        );
    }

    function _sign(uint256 pk, bytes32 digest) internal view returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, digest);
        return abi.encodePacked(r, s, v);
    }

    function _liveSigs(CertLib.MatchLiveCert memory cert) internal view returns (bytes[3] memory) {
        bytes32 d = CertLib.matchLiveDigest(cert);
        return [_sign(counterSessionPk, d), _sign(localSessionPk, d), _sign(operatorPk, d)];
    }

    function _outcome(bytes32 matchId, bytes32 liveDigest, uint8 kind)
        internal
        pure
        returns (CertLib.OutcomeCert memory)
    {
        return CertLib.OutcomeCert({
            matchId: matchId,
            matchLiveDigest: liveDigest,
            outcomeKind: kind,
            stepCount: CertLib.TERMINAL_STEP_COUNT,
            p1Guess: 1,
            p2Guess: 0,
            firstCommitter: 1,
            matchupType: 1,
            transcriptHash: keccak256("transcript")
        });
    }

    function _ocSigs(CertLib.OutcomeCert memory oc) internal view returns (bytes[3] memory) {
        bytes32 d = CertLib.outcomeDigest(oc);
        return [_sign(counterSessionPk, d), _sign(localSessionPk, d), _sign(operatorPk, d)];
    }

    function _settle(bytes32 matchId, bool playerIsP1, uint128 tranche, uint8 kind) internal {
        CertLib.MatchLiveCert memory cert = _cert(matchId, playerIsP1, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.OutcomeCert memory oc = _outcome(matchId, liveDigest, kind);
        game.settle(cert, oc, _liveSigs(cert), _ocSigs(oc));
    }

    /// Read just the status (field 0) of a match without destructuring all
    /// 16 fields.
    function _status(bytes32 matchId) internal view returns (CrossChainGame.Status) {
        (CrossChainGame.Status status,,,,,,,,,,,,,,,,) = game.matches(matchId);
        return status;
    }

    /// The claim window is anchored to matchDeadline (= fund time + 2 days),
    /// not to file time. Tests fund and open claims at ~setUp time, so warp
    /// past matchDeadline + CLAIM_WINDOW to reach the settle window.
    function _warpPastClaimWindow() internal {
        vm.warp(block.timestamp + 2 days + CLAIM_WINDOW + 1);
    }

    // --- state machine ---------------------------------------------------

    function test_happyPath_winnerTakesStakePlusTranche() public {
        bytes32 id = keccak256("m1");
        uint128 tranche = STAKE; // counter-value of opponent stake
        _fund(id, true);
        _lock(id, tranche);

        uint256 before = player.balance;
        // Local player is P1 and wins the heterogeneous match.
        _settle(id, true, tranche, CertLib.HETERO_P1_WINS);

        // Pull-payment (M1): the win is credited, not pushed.
        assertEq(
            player.balance + game.withdrawable(player), before + STAKE + tranche, "winner credited stake + tranche"
        );
        assertEq(uint256(_status(id)), uint256(CrossChainGame.Status.Settled));
    }

    /// Push-preferred payout: a winning EOA is paid INSIDE the settle tx (matching
    /// the Solana xmatch leg, which pushes lamports at settle), so no separate
    /// withdraw is needed and the pull ledger is left empty. Brick-resistance for a
    /// reverting recipient is the fallback, proven in
    /// test_settleFallsBackToLedgerForRevertingWinner.
    function test_winnerIsPaidAtSettle_pushPreferred() public {
        bytes32 id = keccak256("m1-withdraw");
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);

        uint256 before = player.balance;
        _settle(id, true, tranche, CertLib.HETERO_P1_WINS);

        assertEq(player.balance, before + STAKE + tranche, "winner paid at settle, no withdraw needed");
        assertEq(game.withdrawable(player), 0, "nothing left in the pull ledger");

        vm.prank(player);
        vm.expectRevert(PullPayment.NothingToWithdraw.selector);
        game.withdraw();
    }

    /// M4: createMatch requires an operator signature over the exact creation
    /// (matchId bound to msg.sender + session keys + seat + deadlines), so a
    /// griefer can't squat a paired matchId before the assigned player funds.
    function test_M4_matchIdSquatRequiresOperatorSig() public {
        bytes32 id = keccak256("m4-squat");
        address squatter = makeAddr("squatter");
        vm.deal(squatter, 10 ether);
        uint64 fd = uint64(block.timestamp + 1 days);
        uint64 md = uint64(block.timestamp + 2 days);

        // (a) No valid operator authorization → reverts.
        bytes memory badSig = _sign(0xBEEF, keccak256("not the digest"));
        vm.prank(squatter);
        vm.expectRevert(CrossChainGame.BadSignature.selector);
        game.createMatch{value: STAKE}(id, localSession, counterSession, true, fd, md, badSig);

        // (b) A sig the operator signed for the LEGIT player can't be replayed by
        // the squatter — msg.sender is bound into the digest.
        bytes memory legitSig = _createMatchSig(id, player, localSession, counterSession, true, fd, md);
        vm.prank(squatter);
        vm.expectRevert(CrossChainGame.BadSignature.selector);
        game.createMatch{value: STAKE}(id, localSession, counterSession, true, fd, md, legitSig);

        // (c) The assigned player (matching the signed digest) creates the match.
        _fund(id, true);
        assertEq(uint256(_status(id)), uint256(CrossChainGame.Status.Funded), "authorized create succeeds");
    }

    /// L1: setConfig rejects out-of-range stake / tranche / windows.
    function test_L1_setConfigRejectsOutOfRangeValues() public {
        vm.startPrank(owner);
        // stake below the dust floor
        vm.expectRevert(CrossChainGame.BadConfig.selector);
        game.setConfig(operatorSigner, treasury, SPLIT_BPS, 1, MAX_TRANCHE, DAILY_CAP, CLAIM_WINDOW, SKEW);
        // maxTranche over 10x stake
        vm.expectRevert(CrossChainGame.BadConfig.selector);
        game.setConfig(operatorSigner, treasury, SPLIT_BPS, STAKE, STAKE * 11, DAILY_CAP, CLAIM_WINDOW, SKEW);
        // claim window below the 60s floor
        vm.expectRevert(CrossChainGame.BadConfig.selector);
        game.setConfig(operatorSigner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, DAILY_CAP, 59, SKEW);
        // skew over the 30-day cap
        vm.expectRevert(CrossChainGame.BadConfig.selector);
        game.setConfig(operatorSigner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, DAILY_CAP, CLAIM_WINDOW, 31 days);
        vm.stopPrank();
    }

    function test_loserForfeit_splitsTreasuryAndPool() public {
        bytes32 id = keccak256("m2");
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);

        uint256 poolBefore = game.poolFree();
        uint256 treBefore = treasury.balance;
        // Local P1 loses → P2 wins.
        _settle(id, true, tranche, CertLib.HETERO_P2_WINS);

        assertEq(treasury.balance - treBefore + game.withdrawable(treasury), STAKE * SPLIT_BPS / 10000, "treasury cut");
        // Tranche released back + pool reimbursed with the post-treasury remainder.
        uint256 reimburse = STAKE - (STAKE * SPLIT_BPS / 10000);
        assertEq(game.poolFree(), poolBefore + tranche + reimburse, "tranche + reimbursement");
    }

    // --- panel gap: double-claim is structurally impossible --------------

    function test_doubleSettle_sameCert_reverts() public {
        bytes32 id = keccak256("m3");
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);
        _settle(id, true, tranche, CertLib.HETERO_P1_WINS);

        CertLib.MatchLiveCert memory cert = _cert(id, true, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.OutcomeCert memory oc = _outcome(id, liveDigest, CertLib.HETERO_P1_WINS);
        vm.expectRevert(CrossChainGame.InvalidStatus.selector);
        game.settle(cert, oc, _liveSigs(cert), _ocSigs(oc));
    }

    // --- panel gap: cert without a lock is inert -------------------------

    function test_settle_withoutLock_reverts() public {
        bytes32 id = keccak256("m4");
        _fund(id, true); // funded, never locked
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.OutcomeCert memory oc = _outcome(id, liveDigest, CertLib.HETERO_P1_WINS);
        vm.expectRevert(CrossChainGame.InvalidStatus.selector);
        game.settle(cert, oc, _liveSigs(cert), _ocSigs(oc));
    }

    // --- panel gap: cert-term mismatch rejected --------------------------

    function test_settle_wrongStakeInCert_reverts() public {
        bytes32 id = keccak256("m5");
        _fund(id, true);
        _lock(id, STAKE);
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        cert.legB.stake = STAKE + 1; // tampered
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.OutcomeCert memory oc = _outcome(id, liveDigest, CertLib.HETERO_P1_WINS);
        vm.expectRevert(CrossChainGame.CertMismatch.selector);
        game.settle(cert, oc, _liveSigs(cert), _ocSigs(oc));
    }

    function test_settle_wrongChainTag_reverts() public {
        bytes32 id = keccak256("m5b");
        _fund(id, true);
        _lock(id, STAKE);
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        cert.legB.chainTag = keccak256("eip155:1"); // different chain
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.OutcomeCert memory oc = _outcome(id, liveDigest, CertLib.HETERO_P1_WINS);
        vm.expectRevert(CrossChainGame.CertMismatch.selector);
        game.settle(cert, oc, _liveSigs(cert), _ocSigs(oc));
    }

    // --- panel gap: stale quote rejected ---------------------------------

    function test_settle_staleQuote_reverts() public {
        bytes32 id = keccak256("m6");
        _fund(id, true);
        _lock(id, STAKE);
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        // Quote far enough in the past that quote + maxAge < lockedAt.
        cert.quoteTimestamp = uint64(block.timestamp - 1000);
        cert.quoteMaxAgeSecs = 300;
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.OutcomeCert memory oc = _outcome(id, liveDigest, CertLib.HETERO_P1_WINS);
        vm.expectRevert(CrossChainGame.StaleQuote.selector);
        game.settle(cert, oc, _liveSigs(cert), _ocSigs(oc));
    }

    // --- panel gap: bad signature rejected -------------------------------

    function test_settle_forgedOperatorSig_reverts() public {
        bytes32 id = keccak256("m7");
        _fund(id, true);
        _lock(id, STAKE);
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        bytes[3] memory sigs =
            [_sign(counterSessionPk, liveDigest), _sign(localSessionPk, liveDigest), _sign(0xDEAD, liveDigest)];
        CertLib.OutcomeCert memory oc = _outcome(id, liveDigest, CertLib.HETERO_P1_WINS);
        vm.expectRevert(CrossChainGame.BadSignature.selector);
        game.settle(cert, oc, sigs, _ocSigs(oc));
    }

    // --- panel gap: tranche clamp ----------------------------------------

    function test_lockTranche_overMax_reverts() public {
        bytes32 id = keccak256("m8");
        _fund(id, true);
        CertLib.MatchLiveCert memory cert = _cert(id, true, MAX_TRANCHE + 1);
        vm.expectRevert(CrossChainGame.TrancheTooLarge.selector);
        game.lockTranche(cert, _sign(operatorPk, CertLib.matchLiveDigest(cert)));
    }

    function test_lockTranche_overPool_reverts() public {
        // Drain pool to below the tranche. Read poolFree BEFORE the prank
        // so the prank lands on poolWithdraw, not the view call.
        uint256 free = game.poolFree();
        vm.prank(owner);
        game.poolWithdraw(free);
        bytes32 id = keccak256("m8b");
        _fund(id, true);
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        vm.expectRevert(CrossChainGame.PoolInsufficient.selector);
        game.lockTranche(cert, _sign(operatorPk, CertLib.matchLiveDigest(cert)));
    }

    // --- M6: rolling-24h aggregate tranche-lock cap ------------------------

    function test_M6_setConfigRejectsCapBelowMaxTranche() public {
        // The daily cap must admit at least one max-tranche match.
        vm.prank(owner);
        vm.expectRevert(CrossChainGame.BadConfig.selector);
        game.setConfig(operatorSigner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, MAX_TRANCHE - 1, CLAIM_WINDOW, SKEW);
    }

    function test_M6_dailyTrancheCap_blocksExcessLocksInWindow() public {
        // Lower the cap to exactly one max tranche: the first lock fills the
        // day, the second (same window) is rejected — bounding a compromised
        // operator to dailyTrancheCapWei per day.
        vm.prank(owner);
        game.setConfig(operatorSigner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, MAX_TRANCHE, CLAIM_WINDOW, SKEW);

        bytes32 id1 = keccak256("m6a");
        bytes32 id2 = keccak256("m6b");
        _fund(id1, true);
        _fund(id2, true);

        _lock(id1, MAX_TRANCHE); // fills the daily cap
        assertEq(uint256(game.trancheLockedInWindow()), MAX_TRANCHE);

        CertLib.MatchLiveCert memory cert = _cert(id2, true, MAX_TRANCHE);
        vm.expectRevert(CrossChainGame.DailyTrancheCapExceeded.selector);
        game.lockTranche(cert, _sign(operatorPk, CertLib.matchLiveDigest(cert)));
    }

    function test_M6_dailyTrancheCap_resetsAfterWindow() public {
        vm.prank(owner);
        game.setConfig(operatorSigner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, MAX_TRANCHE, CLAIM_WINDOW, SKEW);

        // Fund BOTH matches now with far-future deadlines so they survive a warp
        // past the 24h tranche window (the lock cert binds matchDeadline, so it
        // must equal the fund-time value).
        uint64 t0 = uint64(block.timestamp);
        uint64 md = t0 + 5 days;
        uint64 fd = t0 + 4 days;
        bytes32 id1 = keccak256("m6c");
        bytes32 id2 = keccak256("m6d");
        _fundWithDeadlines(id1, true, fd, md);
        _fundWithDeadlines(id2, true, fd, md);

        // Lock id1 → fills the daily cap for this window.
        CertLib.MatchLiveCert memory c1 = _certMd(id1, true, MAX_TRANCHE, t0, md);
        game.lockTranche(c1, _sign(operatorPk, CertLib.matchLiveDigest(c1)));
        assertEq(uint256(game.trancheLockedInWindow()), MAX_TRANCHE);

        // Roll past the 24h window (still well before the 5-day match deadline):
        // the next lock resets the running total and succeeds. Pass the warped
        // time explicitly as the quote timestamp (vm.warp doesn't update the test
        // frame's own block.timestamp reads).
        uint64 warped = t0 + 1 days + 1;
        vm.warp(warped);
        CertLib.MatchLiveCert memory c2 = _certMd(id2, true, MAX_TRANCHE, warped, md);
        game.lockTranche(c2, _sign(operatorPk, CertLib.matchLiveDigest(c2)));

        assertEq(uint256(game.trancheLockedInWindow()), MAX_TRANCHE, "window reset to a single tranche");
        assertEq(uint256(_status(id2)), uint256(CrossChainGame.Status.Locked));
    }

    // --- permissionless lock: operator-sig authorizes, caller pays gas -----

    function test_lockTranche_permissionless_anyCallerCanLock() public {
        bytes32 id = keccak256("m8c");
        _fund(id, true);
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        bytes memory sig = _sign(operatorPk, CertLib.matchLiveDigest(cert));
        // A random non-owner submits the lock and pays gas — the whole point.
        vm.prank(makeAddr("randomCranker"));
        game.lockTranche(cert, sig);
        assertEq(uint256(_status(id)), uint256(CrossChainGame.Status.Locked));
    }

    function test_lockTranche_badOperatorSig_reverts() public {
        bytes32 id = keccak256("m8d");
        _fund(id, true);
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        // Signed by a session key, not the operator → BadSignature.
        vm.expectRevert(CrossChainGame.BadSignature.selector);
        game.lockTranche(cert, _sign(localSessionPk, CertLib.matchLiveDigest(cert)));
    }

    function test_lockTranche_staleQuote_reverts() public {
        bytes32 id = keccak256("m8e");
        _fund(id, true);
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        // Warp past quoteMaxAgeSecs (300) so the freshness check fails at lock.
        vm.warp(block.timestamp + 301);
        vm.expectRevert(CrossChainGame.StaleQuote.selector);
        game.lockTranche(cert, _sign(operatorPk, CertLib.matchLiveDigest(cert)));
    }

    function test_lockTranche_tamperedTranche_reverts() public {
        // Operator signs tranche=STAKE; a caller submits a cert with a larger
        // tranche. The digest changes, so the operator sig no longer recovers.
        bytes32 id = keccak256("m8f");
        _fund(id, true);
        CertLib.MatchLiveCert memory signed = _cert(id, true, STAKE);
        bytes memory sig = _sign(operatorPk, CertLib.matchLiveDigest(signed));
        CertLib.MatchLiveCert memory tampered = _cert(id, true, MAX_TRANCHE);
        vm.expectRevert(CrossChainGame.BadSignature.selector);
        game.lockTranche(tampered, sig);
    }

    // --- panel gap: refund timing vs skew --------------------------------

    function test_refundNoCert_onlyAfterFundDeadline() public {
        bytes32 id = keccak256("m9");
        _fund(id, true);
        vm.expectRevert(CrossChainGame.DeadlineNotReached.selector);
        game.refundNoCert(id);

        vm.warp(block.timestamp + 1 days + 1);
        uint256 before = player.balance;
        game.refundNoCert(id);
        assertEq(player.balance + game.withdrawable(player), before + STAKE);
    }

    function test_refundTimeout_releasesTrancheAfterSkew() public {
        bytes32 id = keccak256("m10");
        _fund(id, true);
        _lock(id, STAKE);
        uint256 lockedBefore = game.poolLocked();
        assertEq(lockedBefore, STAKE);

        // Too early: matchDeadline + claimWindow + 2*skew not yet reached.
        vm.warp(block.timestamp + 2 days + CLAIM_WINDOW + SKEW);
        vm.expectRevert(CrossChainGame.DeadlineNotReached.selector);
        game.refundTimeout(id);

        vm.warp(block.timestamp + 2 * SKEW);
        uint256 before = player.balance;
        game.refundTimeout(id);
        assertEq(player.balance + game.withdrawable(player), before + STAKE, "stake refunded");
        assertEq(game.poolLocked(), 0, "tranche released");
    }

    // --- claim path ------------------------------------------------------

    function _checkpoint(bytes32 liveDigest, uint8 stepCount, uint8 p1Guess, uint8 p2Guess, uint8 firstCommitter)
        internal
        pure
        returns (CertLib.Checkpoint memory)
    {
        return CertLib.Checkpoint({
            matchLiveDigest: liveDigest,
            stepCount: stepCount,
            p1Commit: keccak256("p1c"),
            p2Commit: keccak256("p2c"),
            p1Guess: p1Guess,
            p2Guess: p2Guess,
            firstCommitter: firstCommitter,
            matchupType: 1,
            transcriptHash: keccak256("t"),
            rMatchup: R_MATCHUP
        });
    }

    function _cpSigs(CertLib.Checkpoint memory cp) internal view returns (bytes[2] memory) {
        bytes32 d = CertLib.checkpointDigest(cp);
        return [_sign(counterSessionPk, d), _sign(localSessionPk, d)];
    }

    /// M2: the claim window is snapshotted at lock, so an owner setConfig that
    /// lowers maxClaimWindowSecs below the locked cert's claimWindowSecs cannot
    /// DoS a valid openClaim (which previously re-validated against live config).
    function test_M2_setConfigCannotDosOpenClaim() public {
        bytes32 id = keccak256("m2-dos");
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);

        // Shrink maxClaimWindowSecs to the floor (60), far below the locked cert's
        // claimWindowSecs (CLAIM_WINDOW=3600).
        vm.prank(owner);
        game.setConfig(operatorSigner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, DAILY_CAP, 60, SKEW);

        CertLib.MatchLiveCert memory cert = _cert(id, true, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.Checkpoint memory cp = _checkpoint(liveDigest, 1, 255, 255, 1);
        // Pre-M2 this reverted BadConfig (cert.claimWindowSecs > live max); now
        // it uses the lock-time snapshot and succeeds.
        game.openClaim(cert, cp, _liveSigs(cert), _cpSigs(cp));
        assertEq(uint256(_status(id)), uint256(CrossChainGame.Status.Claiming), "claim opened despite shrunk config");
    }

    function test_claimPath_committerWinsAfterWindow() public {
        bytes32 id = keccak256("m11");
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);

        CertLib.MatchLiveCert memory cert = _cert(id, true, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        // step 1: only first committer (P1 = local) committed.
        CertLib.Checkpoint memory cp = _checkpoint(liveDigest, 1, 255, 255, 1);
        game.openClaim(cert, cp, _liveSigs(cert), _cpSigs(cp));

        vm.expectRevert(CrossChainGame.DeadlineNotReached.selector);
        game.settleClaim(id);

        _warpPastClaimWindow();
        uint256 before = player.balance;
        game.settleClaim(id);
        // P1 (local) wins both stakes: own stake + tranche (credited).
        assertEq(player.balance + game.withdrawable(player), before + STAKE + tranche);
    }

    function test_supersede_higherStepCountReplacesClaim() public {
        bytes32 id = keccak256("m12");
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);
        CertLib.MatchLiveCert memory cert = _cert(id, true, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);

        // Open with step 1 (P1 committer wins).
        CertLib.Checkpoint memory cp1 = _checkpoint(liveDigest, 1, 255, 255, 1);
        game.openClaim(cert, cp1, _liveSigs(cert), _cpSigs(cp1));

        // Supersede with terminal step 4 where P2 actually won.
        CertLib.Checkpoint memory cp4 = _checkpoint(liveDigest, 4, 0, 1, 1); // p2 correct
        game.supersedeClaim(cert, cp4, _cpSigs(cp4));

        _warpPastClaimWindow();
        uint256 before = player.balance;
        game.settleClaim(id);
        // Local P1 lost the superseding terminal outcome → no payout.
        assertEq(player.balance, before, "superseded to a loss");
    }

    function test_supersede_lowerStepCount_reverts() public {
        bytes32 id = keccak256("m13");
        _fund(id, true);
        _lock(id, STAKE);
        CertLib.MatchLiveCert memory cert = _cert(id, true, STAKE);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.Checkpoint memory cp3 = _checkpoint(liveDigest, 3, 1, 255, 1);
        game.openClaim(cert, cp3, _liveSigs(cert), _cpSigs(cp3));
        CertLib.Checkpoint memory cp1 = _checkpoint(liveDigest, 1, 255, 255, 1);
        vm.expectRevert(CrossChainGame.BadOutcome.selector);
        game.supersedeClaim(cert, cp1, _cpSigs(cp1));
    }

    // --- equivocation ----------------------------------------------------

    function test_equivocation_localPlayerForfeits() public {
        bytes32 id = keccak256("m14");
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);
        CertLib.MatchLiveCert memory cert = _cert(id, true, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);

        // Two distinct step-2 checkpoints both signed by the local session key.
        CertLib.Checkpoint memory a = _checkpoint(liveDigest, 2, 255, 255, 1);
        CertLib.Checkpoint memory b = _checkpoint(liveDigest, 2, 255, 255, 2); // differs
        bytes memory sigA = _sign(localSessionPk, CertLib.checkpointDigest(a));
        bytes memory sigB = _sign(localSessionPk, CertLib.checkpointDigest(b));

        game.submitEquivocationProof(cert, a, b, sigA, sigB);
        _warpPastClaimWindow();
        uint256 before = player.balance;
        game.settleClaim(id);
        // Local P1 equivocated → forfeits; no payout.
        assertEq(player.balance, before, "equivocator forfeits");
    }

    /// Regression (review finding): when BOTH parties equivocate, the
    /// verdict must be both-forfeit REGARDLESS of proof order. The old code
    /// inferred prior equivocation from bestOutcomeKind, so counterparty-then-
    /// local order wrongly left the counterparty as outright winner.
    function test_equivocation_bothForfeit_orderIndependent() public {
        _assertBothEquivocateForfeit(keccak256("eqA"), true); // counter first
        _assertBothEquivocateForfeit(keccak256("eqB"), false); // local first
    }

    function _assertBothEquivocateForfeit(bytes32 id, bool counterFirst) internal {
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);
        CertLib.MatchLiveCert memory cert = _cert(id, true, tranche);
        bytes32 ld = CertLib.matchLiveDigest(cert);

        CertLib.Checkpoint memory a = _checkpoint(ld, 2, 255, 255, 1);
        CertLib.Checkpoint memory b = _checkpoint(ld, 2, 255, 255, 2);
        bytes memory localA = _sign(localSessionPk, CertLib.checkpointDigest(a));
        bytes memory localB = _sign(localSessionPk, CertLib.checkpointDigest(b));
        bytes memory counterA = _sign(counterSessionPk, CertLib.checkpointDigest(a));
        bytes memory counterB = _sign(counterSessionPk, CertLib.checkpointDigest(b));

        if (counterFirst) {
            game.submitEquivocationProof(cert, a, b, counterA, counterB);
            game.submitEquivocationProof(cert, a, b, localA, localB);
        } else {
            game.submitEquivocationProof(cert, a, b, localA, localB);
            game.submitEquivocationProof(cert, a, b, counterA, counterB);
        }
        _warpPastClaimWindow();
        // This helper runs twice in one test, so credits ACCUMULATE — measure the
        // claimable (balance + withdrawable) DELTA from this settle, not the total.
        uint256 treBefore = treasury.balance + game.withdrawable(treasury);
        uint256 before = player.balance + game.withdrawable(player);
        game.settleClaim(id);
        // Both forfeit: player gets nothing, the whole stake splits to
        // treasury+prize, and the tranche returns to the pool.
        assertEq(player.balance + game.withdrawable(player), before, "both-forfeit pays player nothing");
        assertEq(
            treasury.balance + game.withdrawable(treasury) - treBefore,
            STAKE * SPLIT_BPS / 10000,
            "treasury cut on forfeit"
        );
    }

    /// Regression (review finding): an honest player who legitimately filed a
    /// claim that currently shows them LOSING on timeout must still WIN if the
    /// counterparty is then proven to equivocate — the old localAlreadySlashed
    /// heuristic wrongly downgraded this to both-forfeit.
    function test_counterEquivocation_afterLosingClaim_localStillWins() public {
        bytes32 id = keccak256("eqC");
        uint128 tranche = STAKE;
        _fund(id, true); // local is P1
        _lock(id, tranche);
        CertLib.MatchLiveCert memory cert = _cert(id, true, tranche);
        bytes32 ld = CertLib.matchLiveDigest(cert);

        // Open a claim that shows local (P1) losing: step-1 where the
        // counterparty (P2) was the sole committer → TIMEOUT_P2_WINS.
        CertLib.Checkpoint memory losing = _checkpoint(ld, 1, 255, 255, 2);
        game.openClaim(cert, losing, _liveSigs(cert), _cpSigs(losing));

        // Now prove the COUNTERPARTY equivocated.
        CertLib.Checkpoint memory a = _checkpoint(ld, 2, 255, 255, 1);
        CertLib.Checkpoint memory b = _checkpoint(ld, 2, 255, 255, 2);
        game.submitEquivocationProof(
            cert,
            a,
            b,
            _sign(counterSessionPk, CertLib.checkpointDigest(a)),
            _sign(counterSessionPk, CertLib.checkpointDigest(b))
        );

        _warpPastClaimWindow();
        uint256 before = player.balance;
        game.settleClaim(id);
        // Local P1 should WIN outright: own stake + tranche.
        assertEq(
            player.balance + game.withdrawable(player),
            before + STAKE + tranche,
            "honest local wins on counter-equivocation"
        );
    }

    // --- fuzz: every terminal outcome conserves value --------------------

    function testFuzz_conservation_acrossAllOutcomes(uint8 rawKind, bool playerIsP1, uint128 trancheSeed) public {
        uint8 kind = rawKind % 9; // 0..8 valid outcome kinds
        uint128 tranche = uint128(bound(trancheSeed, 0, MAX_TRANCHE));
        bytes32 id = keccak256(abi.encode(rawKind, playerIsP1, trancheSeed));

        _fund(id, playerIsP1);
        _lock(id, tranche);

        uint256 contractBefore = address(game).balance;

        CertLib.MatchLiveCert memory cert = _cert(id, playerIsP1, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        CertLib.OutcomeCert memory oc = _outcome(id, liveDigest, kind);
        oc.stepCount = CertLib.TERMINAL_STEP_COUNT;
        game.settle(cert, oc, _liveSigs(cert), _ocSigs(oc));

        // Push-preferred (M1): settle now PUSHES an EOA player's payout out of the
        // contract immediately (`player` is an EOA, so it is pushed, not credited;
        // a reverting recipient would fall back to the ledger instead). Whatever
        // was pushed left BOTH the held ETH and the ledger, so the tracked-balance
        // reconciliation still holds exactly across held + pushed.
        assertLe(address(game).balance, contractBefore, "settle never GAINS ether");
        assertEq(
            address(game).balance,
            game.poolFree() + game.poolLocked() + game.prizePoolWei() + game.withdrawable(player)
                + game.withdrawable(treasury),
            "tracked balances reconcile (held ETH == pool + prize + credits; a pushed payout already left)"
        );
    }

    // --- regression: audit findings (2026-06-17) -------------------------

    /// Finding #1: after the backstop opens, BOTH settleClaim and refundTimeout
    /// are live on a Claiming match. refundTimeout must realize the proven claim
    /// (stake + tranche for a winner), never a flat stake-only refund that a
    /// griefer could use to strip the winner's tranche back into the pool.
    function test_refundTimeout_onClaiming_realizesClaimNotFlatRefund() public {
        bytes32 id = keccak256("m_claimrace");
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);

        CertLib.MatchLiveCert memory cert = _cert(id, true, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        // P1 (local) is the sole committer → committer wins both stakes.
        CertLib.Checkpoint memory cp = _checkpoint(liveDigest, 1, 255, 255, 1);
        game.openClaim(cert, cp, _liveSigs(cert), _cpSigs(cp));

        // Warp past the backstop so refundTimeout is also callable.
        vm.warp(block.timestamp + 2 days + CLAIM_WINDOW + 2 * SKEW + 1);

        // A griefer (not the winner) calls refundTimeout instead of settleClaim.
        uint256 before = player.balance;
        vm.prank(makeAddr("griefer"));
        game.refundTimeout(id);

        assertEq(
            player.balance + game.withdrawable(player),
            before + STAKE + tranche,
            "winning claim realized, tranche not stripped"
        );
        assertEq(uint256(_status(id)), uint256(CrossChainGame.Status.ClaimSettled));
    }

    /// Finding #2: the refund backstop is snapshotted at lock time, so a later
    /// setConfig that lowers maxClaimWindowSecs/skew cannot pull refundTimeout
    /// ahead of a still-open claim window.
    function test_setConfig_cannotPullRefundTimeoutBeforeSnapshot() public {
        bytes32 id = keccak256("m_setconfig");
        _fund(id, true);
        _lock(id, STAKE);
        uint256 matchDeadline = block.timestamp + 2 days; // _fund used this
        uint256 snapshotOpensAt = matchDeadline + CLAIM_WINDOW + 2 * SKEW;

        // Owner shrinks window + skew to the floor (60) AFTER the lock.
        vm.prank(owner);
        game.setConfig(operatorSigner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, DAILY_CAP, 60, 60);

        // A non-snapshotted opensAt would be matchDeadline + 60 + 2*60 = +180; warp
        // past that but before the snapshot (matchDeadline + 3600 + 1800) — the
        // snapshot still gates the refund.
        vm.warp(matchDeadline + 1000);
        vm.expectRevert(CrossChainGame.DeadlineNotReached.selector);
        game.refundTimeout(id);

        vm.warp(snapshotOpensAt + 1);
        uint256 before = player.balance;
        game.refundTimeout(id);
        assertEq(
            player.balance + game.withdrawable(player), before + STAKE, "refund only after the snapshotted backstop"
        );
    }

    /// Finding #4: key separation enforced — the certificate signer can never be
    /// the owner/upgrade authority or the treasury.
    function test_constructor_rejectsOperatorEqualsOwner() public {
        vm.expectRevert(CrossChainGame.BadConfig.selector);
        new CrossChainGame(
            CHAIN_TAG, owner, owner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, DAILY_CAP, CLAIM_WINDOW, SKEW
        );
    }

    function test_constructor_rejectsOperatorEqualsTreasury() public {
        vm.expectRevert(CrossChainGame.BadConfig.selector);
        new CrossChainGame(
            CHAIN_TAG, owner, treasury, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, DAILY_CAP, CLAIM_WINDOW, SKEW
        );
    }

    function test_setConfig_rejectsOperatorEqualsOwner() public {
        vm.prank(owner);
        vm.expectRevert(CrossChainGame.BadConfig.selector);
        game.setConfig(owner, treasury, SPLIT_BPS, STAKE, MAX_TRANCHE, DAILY_CAP, CLAIM_WINDOW, SKEW);
    }

    /// Finding #5: on the operator-less contested path, a TERMINAL claim
    /// checkpoint whose matchup is not bound to the cert's commitment is
    /// rejected — two colluding players cannot settle a fabricated matchup.
    function test_openClaim_unboundMatchup_reverts() public {
        bytes32 id = keccak256("m_matchupbind");
        uint128 tranche = STAKE;
        _fund(id, true);
        _lock(id, tranche);

        CertLib.MatchLiveCert memory cert = _cert(id, true, tranche);
        bytes32 liveDigest = CertLib.matchLiveDigest(cert);
        // Terminal checkpoint with a forged rMatchup that does NOT open the
        // commitment (both players co-sign it, but the binding check fails).
        CertLib.Checkpoint memory cp = _checkpoint(liveDigest, 4, 0, 1, 1);
        cp.rMatchup = bytes32(uint256(0xBEEF));
        vm.expectRevert(CrossChainGame.CertMismatch.selector);
        game.openClaim(cert, cp, _liveSigs(cert), _cpSigs(cp));
    }
}
