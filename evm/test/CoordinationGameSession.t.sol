// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CoordinationGame} from "../src/CoordinationGame.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @notice Session-authorization tests — the EVM port of Solana's
///         SessionAuthority. The wallet stakes once (createGame/joinGame stay
///         payable + wallet-called) and authorizes a gas-only session key that
///         drives the non-payable commit/reveal; payouts still land on the
///         wallet via the wallet-keyed pull-payment ledger, so there is no
///         sweep-back and no stranding. Mirrors the Solana e2e where a separate
///         session key signs commit/reveal while escrow stays wallet-bound.
contract CoordinationGameSessionTest is Test {
    CoordinationGame internal game;

    uint256 internal constant operatorPk = 0xA11CE;
    address internal operator;
    address internal owner = address(0xB0B);
    address internal treasury = address(0x7EA5);

    // Wallets are the on-chain players/stakers (need keys to sign auth).
    uint256 internal constant walletPk1 = 0xA71;
    uint256 internal constant walletPk2 = 0xA72;
    uint256 internal constant sessionPk1 = 0x5E5510;
    address internal wallet1;
    address internal wallet2;
    address internal sessionKey1;

    uint128 internal constant STAKE = 0.05 ether;
    uint16 internal constant SPLIT_BPS = 5000;
    uint32 internal constant COMMIT_TIMEOUT = 3600;
    uint32 internal constant REVEAL_TIMEOUT = 7200;

    function setUp() public {
        vm.warp(1_765_000_000);
        operator = vm.addr(operatorPk);
        wallet1 = vm.addr(walletPk1);
        wallet2 = vm.addr(walletPk2);
        sessionKey1 = vm.addr(sessionPk1);
        vm.prank(owner);
        game = new CoordinationGame(owner, operator, treasury, SPLIT_BPS, STAKE, COMMIT_TIMEOUT, REVEAL_TIMEOUT);
        vm.deal(wallet1, 100 ether);
        vm.deal(wallet2, 100 ether);
        vm.deal(sessionKey1, 1 ether); // gas dust only — never stakes
    }

    // ----- helpers --------------------------------------------------------

    function _withBit(bytes32 salt, uint8 bit) internal pure returns (bytes32) {
        return bytes32((uint256(salt) & ~uint256(1)) | uint256(bit & 1));
    }

    function _commit(uint8 guess, bytes32 salt) internal pure returns (bytes32 r, bytes32 commitment) {
        r = _withBit(salt, guess);
        commitment = sha256(abi.encodePacked(r));
    }

    function _opSigFor(bytes32 gameId, address creator, bytes32 commitment) internal view returns (bytes memory) {
        bytes32 digest = keccak256(abi.encode(block.chainid, address(game), gameId, creator, commitment));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(operatorPk, digest);
        return abi.encodePacked(r, s, v);
    }

    /// The wallet's off-chain personal_sign over the domain-bound session digest.
    function _sessionSig(uint256 walletPk, address player, address sessionKey, uint64 expiry)
        internal
        view
        returns (bytes memory)
    {
        bytes32 digest = game.sessionAuthDigest(player, sessionKey, expiry);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(walletPk, MessageHashUtils.toEthSignedMessageHash(digest));
        return abi.encodePacked(r, s, v);
    }

    /// Stake a hetero game: wallet1 creates, wallet2 joins. Returns rMatchup.
    function _stakeHetero(bytes32 gameId) internal returns (bytes32 rMatchup) {
        bytes32 mc;
        (rMatchup, mc) = _commit(1, keccak256(abi.encode(gameId, "matchup")));
        vm.prank(wallet1);
        game.createGame{value: STAKE}(gameId, mc, _opSigFor(gameId, wallet1, mc));
        vm.prank(wallet2);
        game.joinGame{value: STAKE}(gameId);
    }

    function _status(bytes32 gameId) internal view returns (CoordinationGame.Status s) {
        (s,,,,,,,,,,,,,,,,) = game.games(gameId);
    }

    // ----- the load-bearing test: session key drives play, wallet is paid --

    /// The full wallet-as-player flow: wallet1 stakes once and authorizes a
    /// session key; that key (holding only gas dust) drives commit + reveal;
    /// wallet1 wins the hetero pot and the payout is credited to WALLET1, never
    /// the session key. The session key's ETH balance is untouched by staking.
    function test_session_keyDrivesPlay_walletIsPaid() public {
        bytes32 gameId = keccak256("session-happy");
        bytes32 rMatchup = _stakeHetero(gameId);

        uint64 expiry = uint64(block.timestamp + 1 days);
        // Anyone may submit the wallet's signed authorization; the session key does.
        vm.prank(sessionKey1);
        game.authorizeSession(wallet1, sessionKey1, expiry, _sessionSig(walletPk1, wallet1, sessionKey1, expiry));

        // Hetero, wallet1 (p1) guesses correctly → wins the whole pot.
        (bytes32 r1, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        (bytes32 r2, bytes32 c2) = _commit(0, keccak256(abi.encode(gameId, "2")));

        uint256 sessionBalBefore = sessionKey1.balance;

        // wallet1's seat is driven ENTIRELY by the session key (non-payable).
        vm.prank(sessionKey1);
        game.commitGuess(gameId, c1);
        vm.prank(wallet2);
        game.commitGuess(gameId, c2);

        vm.prank(sessionKey1);
        game.revealGuess(gameId, r1, rMatchup);
        vm.prank(wallet2);
        game.revealGuess(gameId, r2, bytes32(0));

        assertEq(uint8(_status(gameId)), uint8(CoordinationGame.Status.Resolved), "resolved");
        // Payout credited to the WALLET, not the session key.
        assertEq(game.withdrawable(wallet1), 2 * uint256(STAKE), "wallet1 credited the full pot");
        assertEq(game.withdrawable(sessionKey1), 0, "session key credited nothing");

        // The session key never staked — only spent gas (zero in Foundry EOAs).
        assertEq(sessionKey1.balance, sessionBalBefore, "session key balance untouched by staking");

        uint256 before = wallet1.balance;
        vm.prank(wallet1);
        game.withdraw();
        assertEq(wallet1.balance, before + 2 * uint256(STAKE), "wallet1 realizes the pot");
    }

    /// The wallet can still act directly even after authorizing a session key —
    /// backward compatibility with the msg.sender==player path.
    function test_session_walletStillActsDirectly() public {
        bytes32 gameId = keccak256("session-wallet-direct");
        bytes32 rMatchup = _stakeHetero(gameId);

        uint64 expiry = uint64(block.timestamp + 1 days);
        vm.prank(wallet1);
        game.authorizeSession(wallet1, sessionKey1, expiry, _sessionSig(walletPk1, wallet1, sessionKey1, expiry));

        (bytes32 r1, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        (bytes32 r2, bytes32 c2) = _commit(0, keccak256(abi.encode(gameId, "2")));

        // wallet1 acts directly; wallet2 acts directly.
        vm.prank(wallet1);
        game.commitGuess(gameId, c1);
        vm.prank(wallet2);
        game.commitGuess(gameId, c2);
        vm.prank(wallet1);
        game.revealGuess(gameId, r1, rMatchup);
        vm.prank(wallet2);
        game.revealGuess(gameId, r2, bytes32(0));

        assertEq(game.withdrawable(wallet1), 2 * uint256(STAKE), "wallet1 wins acting directly");
    }

    // ----- authorization rejections --------------------------------------

    /// A signature from the wrong key can't authorize a session for the wallet.
    function test_session_rejectsBadWalletSig() public {
        uint64 expiry = uint64(block.timestamp + 1 days);
        // Sign the wallet1 digest with wallet2's key.
        bytes memory badSig = _sessionSig(walletPk2, wallet1, sessionKey1, expiry);
        vm.expectRevert(CoordinationGame.BadSignature.selector);
        game.authorizeSession(wallet1, sessionKey1, expiry, badSig);
    }

    /// An already-expired authorization is rejected at registration.
    function test_session_rejectsPastExpiry() public {
        uint64 expiry = uint64(block.timestamp); // not strictly future
        bytes memory sig = _sessionSig(walletPk1, wallet1, sessionKey1, expiry);
        vm.expectRevert(CoordinationGame.BadSession.selector);
        game.authorizeSession(wallet1, sessionKey1, expiry, sig);
    }

    /// The zero address can't be authorized as a session key.
    function test_session_rejectsZeroSessionKey() public {
        uint64 expiry = uint64(block.timestamp + 1 days);
        bytes memory sig = _sessionSig(walletPk1, wallet1, address(0), expiry);
        vm.expectRevert(CoordinationGame.BadSession.selector);
        game.authorizeSession(wallet1, address(0), expiry, sig);
    }

    // ----- expiry, revocation, replay ------------------------------------

    /// After the session expires, the key can no longer act — the game sees it
    /// as an unrelated address and reverts NotParticipant.
    function test_session_expiryStopsTheKey() public {
        bytes32 gameId = keccak256("session-expiry");
        _stakeHetero(gameId);

        uint64 expiry = uint64(block.timestamp + 1 days);
        vm.prank(wallet1);
        game.authorizeSession(wallet1, sessionKey1, expiry, _sessionSig(walletPk1, wallet1, sessionKey1, expiry));

        // Jump past expiry.
        vm.warp(uint256(expiry) + 1);
        (, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        vm.prank(sessionKey1);
        vm.expectRevert(CoordinationGame.NotParticipant.selector);
        game.commitGuess(gameId, c1);
    }

    /// Revocation immediately severs the session key's authority.
    function test_session_revokeStopsTheKey() public {
        bytes32 gameId = keccak256("session-revoke");
        _stakeHetero(gameId);

        uint64 expiry = uint64(block.timestamp + 1 days);
        vm.prank(wallet1);
        game.authorizeSession(wallet1, sessionKey1, expiry, _sessionSig(walletPk1, wallet1, sessionKey1, expiry));

        vm.prank(wallet1);
        game.revokeSession();

        (, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        vm.prank(sessionKey1);
        vm.expectRevert(CoordinationGame.NotParticipant.selector);
        game.commitGuess(gameId, c1);
    }

    /// An authorization signature can't be replayed after the nonce moves — a
    /// second authorize (or a revoke) bumps sessionNonce, invalidating the old
    /// signed digest.
    function test_session_authSigNotReplayableAfterNonceBump() public {
        uint64 expiry = uint64(block.timestamp + 1 days);
        bytes memory sig0 = _sessionSig(walletPk1, wallet1, sessionKey1, expiry);

        // First use consumes nonce 0.
        vm.prank(wallet1);
        game.authorizeSession(wallet1, sessionKey1, expiry, sig0);

        // Revoke bumps the nonce again; the original sig now binds a stale nonce.
        vm.prank(wallet1);
        game.revokeSession();

        vm.expectRevert(CoordinationGame.BadSignature.selector);
        game.authorizeSession(wallet1, sessionKey1, expiry, sig0);
    }

    /// A key authorized for wallet1 cannot act for wallet2's seat.
    function test_session_keyBoundToItsWalletOnly() public {
        bytes32 gameId = keccak256("session-cross");
        _stakeHetero(gameId);

        uint64 expiry = uint64(block.timestamp + 1 days);
        // sessionKey1 is authorized for wallet1 only.
        vm.prank(wallet1);
        game.authorizeSession(wallet1, sessionKey1, expiry, _sessionSig(walletPk1, wallet1, sessionKey1, expiry));

        // wallet2 has NOT authorized sessionKey1; it commits wallet1's seat (p1),
        // never wallet2's. Prove it lands on p1 by having wallet2's own commit be
        // distinct and checking the p1 slot is the one filled.
        (, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        vm.prank(sessionKey1);
        game.commitGuess(gameId, c1);

        // The p1 commitment is set; p2 is still empty (sessionKey1 acted for p1).
        (,,,,,,,,, bytes32 p1Commit, bytes32 p2Commit,,,,,,) = game.games(gameId);
        assertEq(p1Commit, c1, "session key filled wallet1's (p1) seat");
        assertEq(p2Commit, bytes32(0), "wallet2's (p2) seat untouched");
    }

    /// A wholly unrelated address (no authorization) can't drive either seat.
    function test_session_unauthorizedThirdPartyRejected() public {
        bytes32 gameId = keccak256("session-stranger");
        _stakeHetero(gameId);

        (, bytes32 c1) = _commit(1, keccak256(abi.encode(gameId, "1")));
        vm.prank(address(0xDEAD));
        vm.expectRevert(CoordinationGame.NotParticipant.selector);
        game.commitGuess(gameId, c1);
    }
}
