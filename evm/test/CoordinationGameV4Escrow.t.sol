// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CoordinationGameV4} from "../src/CoordinationGameV4.sol";
import {ERC1967Proxy} from "../lib/openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";

/// Escrow parity with Solana (v5): the stake may be staged in the contract's
/// own ledger instead of resting in the ephemeral session key.
///
/// WHY THIS MATTERS. On Solana `deposit_stake` moves the stake into a
/// program-owned `StakeEscrow` PDA and `withdraw_stake` returns it on the
/// player's signature. On EVM the stake sat in the SESSION KEY between
/// `openSession` and `createGame` — so a lost or expired session key took the
/// ETH with it, with no recovery path. That is a custody difference, not a UX
/// difference, and it is the one thing the two chains did not agree on.
///
/// The ledger is the EXISTING `PullPayment.withdrawable`, not a new mapping.
/// That is what makes this a logic-only upgrade behind the ERC1967 proxy: no
/// storage slot moves, so a live contract holding real funds keeps its seasons,
/// player records, and unclaimed balances untouched.
contract CoordinationGameV4EscrowTest is Test {
    CoordinationGameV4 internal game;

    uint256 internal constant operatorPk = 0xA11CE;
    address internal owner = address(0x0FFE);
    address internal operator;
    address internal treasury = address(0xCAFE);

    address internal alice = address(0xA1);
    address internal bob = address(0xB0B);
    address internal mallory = address(0xBAD);

    uint256 internal constant STAKE = 0.0027 ether;

    function setUp() public {
        operator = vm.addr(operatorPk);
        CoordinationGameV4 impl = new CoordinationGameV4();
        bytes memory init = abi.encodeCall(
            CoordinationGameV4.initialize, (owner, operator, treasury, 5000, uint128(STAKE), 3600, 7200, 1, 365 days)
        );
        game = CoordinationGameV4(payable(address(new ERC1967Proxy(address(impl), init))));
        vm.deal(alice, 10 ether);
        vm.deal(bob, 10 ether);
        vm.deal(mallory, 10 ether);
    }

    function _opSig(bytes32 gameId, bytes32 commitment, address creator) internal view returns (bytes memory) {
        bytes32 digest = keccak256(abi.encode(block.chainid, address(game), gameId, creator, commitment));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(operatorPk, digest);
        return abi.encodePacked(r, s, v);
    }

    function _commitment(bytes32 gameId) internal pure returns (bytes32) {
        return keccak256(abi.encode(gameId, "matchup"));
    }

    // ----- deposit ---------------------------------------------------------

    function test_depositCreditsTheLedger() public {
        vm.prank(alice);
        game.deposit{value: 1 ether}();
        assertEq(game.withdrawable(alice), 1 ether, "deposit credits the player's own balance");
        assertEq(address(game).balance, 1 ether, "the ETH is held by the contract, not a session key");
    }

    /// The recovery property the whole change exists for: a staged stake is
    /// always reclaimable by the player. ETH left in a lost session key is not.
    function test_aStagedStakeCanBeWithdrawnAgain() public {
        vm.prank(alice);
        game.deposit{value: STAKE}();
        uint256 before = alice.balance;

        vm.prank(alice);
        game.withdraw();

        assertEq(alice.balance, before + STAKE, "player recovered the staged stake");
        assertEq(game.withdrawable(alice), 0);
    }

    function test_zeroValueDepositIsRejected() public {
        // Reject at the boundary rather than emitting a meaningless event.
        vm.prank(alice);
        vm.expectRevert(CoordinationGameV4.BadStake.selector);
        game.deposit{value: 0}();
    }

    // ----- staking from the ledger ----------------------------------------

    function test_createGameDebitsTheLedgerWhenNoValueIsSent() public {
        bytes32 gameId = keccak256("g-escrow");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.deposit{value: 1 ether}();

        vm.prank(alice);
        game.createGame(gameId, mc, _opSig(gameId, mc, alice), alice);

        assertEq(game.withdrawable(alice), 1 ether - STAKE, "exactly the stake was debited");
        (, address p1,,) = _game(gameId);
        assertEq(p1, alice, "the wallet is the recorded player, not the payer");
    }

    /// The game must record the CONFIG stake, not `msg.value` — on the escrow
    /// path msg.value is 0, and a game carrying stakeWei == 0 would settle every
    /// payout to nothing while looking perfectly healthy.
    function test_escrowPathRecordsTheRealStakeOnTheGame() public {
        bytes32 gameId = keccak256("g-stake-recorded");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.deposit{value: 1 ether}();
        vm.prank(alice);
        game.createGame(gameId, mc, _opSig(gameId, mc, alice), alice);

        (,,, uint128 stakeRecorded) = _game(gameId);
        assertEq(uint256(stakeRecorded), STAKE, "game.stakeWei must be the config stake, not msg.value");
    }

    function test_joinGameDebitsTheLedgerToo() public {
        bytes32 gameId = keccak256("g-join-escrow");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.createGame{value: STAKE}(gameId, mc, _opSig(gameId, mc, alice), alice);

        vm.prank(bob);
        game.deposit{value: STAKE}();
        vm.prank(bob);
        game.joinGame(gameId, bob);

        assertEq(game.withdrawable(bob), 0, "joiner's balance funded the stake");
        assertEq(address(game).balance, 2 * STAKE, "both stakes are held by the contract");
    }

    function test_stakingWithTooSmallABalanceReverts() public {
        bytes32 gameId = keccak256("g-short");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.deposit{value: STAKE - 1}();

        vm.prank(alice);
        vm.expectRevert(CoordinationGameV4.BadStake.selector);
        game.createGame(gameId, mc, _opSig(gameId, mc, alice), alice);

        assertEq(game.withdrawable(alice), STAKE - 1, "a failed stake leaves the balance untouched");
    }

    function test_noBalanceAndNoValueReverts() public {
        bytes32 gameId = keccak256("g-nothing");
        bytes32 mc = _commitment(gameId);
        vm.prank(alice);
        vm.expectRevert(CoordinationGameV4.BadStake.selector);
        game.createGame(gameId, mc, _opSig(gameId, mc, alice), alice);
    }

    /// A partial `msg.value` must NOT be topped up from the balance. Mixing the
    /// sources would make the amount actually taken depend on a balance the
    /// caller never checked — the classic "accepted invalid input" failure.
    function test_partialValueIsRejectedRatherThanToppedUp() public {
        bytes32 gameId = keccak256("g-partial");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.deposit{value: 1 ether}();

        vm.prank(alice);
        vm.expectRevert(CoordinationGameV4.BadStake.selector);
        game.createGame{value: STAKE - 1}(gameId, mc, _opSig(gameId, mc, alice), alice);

        assertEq(game.withdrawable(alice), 1 ether, "no partial debit occurred");
    }

    /// Overpaying was already rejected and must stay rejected — otherwise the
    /// surplus is silently absorbed by the contract with no way to reclaim it.
    function test_overpaymentIsStillRejected() public {
        bytes32 gameId = keccak256("g-over");
        bytes32 mc = _commitment(gameId);
        vm.prank(alice);
        vm.expectRevert(CoordinationGameV4.BadStake.selector);
        game.createGame{value: STAKE + 1}(gameId, mc, _opSig(gameId, mc, alice), alice);
    }

    // ----- openSessionAndDeposit: one popup, escrowed stake ----------------

    uint64 internal constant SESSION_TTL = 1 days;
    address internal sessionKey = address(0x5E55);

    function _expiry() internal view returns (uint64) {
        return uint64(block.timestamp) + SESSION_TTL;
    }

    /// The property the whole v6 change exists for: after opening a session the
    /// session EOA holds ONLY gas, and the stake is in the player's ledger.
    function test_openSessionAndDepositSplitsGasFromStake() public {
        uint256 gas = 0.0003 ether;

        vm.prank(alice);
        game.openSessionAndDeposit{value: gas + STAKE}(sessionKey, _expiry(), gas);

        assertEq(sessionKey.balance, gas, "session EOA must hold ONLY the gas buffer");
        assertEq(game.withdrawable(alice), STAKE, "the stake is escrowed, not in the session key");
    }

    /// And that stake must be immediately usable, with msg.value == 0.
    function test_theDepositedStakeIsSpendableByTheSessionKey() public {
        uint256 gas = 0.0003 ether;
        bytes32 gameId = keccak256("g-v6");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.openSessionAndDeposit{value: gas + STAKE}(sessionKey, _expiry(), gas);

        // The SESSION KEY acts for alice and sends no value; the stake comes
        // from alice's ledger. This is the whole browser flow in one call.
        vm.prank(sessionKey);
        game.createGame(gameId, mc, _opSig(gameId, mc, alice), alice);

        assertEq(game.withdrawable(alice), 0, "the escrowed stake funded the game");
        (, address p1,, uint128 stakeRecorded) = _game(gameId);
        assertEq(p1, alice, "the WALLET is the player, not the session key");
        assertEq(uint256(stakeRecorded), STAKE);
    }

    function test_gasAmountAboveValueReverts() public {
        vm.prank(alice);
        vm.expectRevert(CoordinationGameV4.BadStake.selector);
        game.openSessionAndDeposit{value: 1 ether}(sessionKey, _expiry(), 1 ether + 1);
    }

    /// Gas-only is legal: it is exactly `openSession`, reached through the new
    /// entry point. Nothing is credited, so a later stake must still revert.
    function test_gasOnlyCreditsNothing() public {
        uint256 gas = 0.0003 ether;
        bytes32 gameId = keccak256("g-v6-nostake");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.openSessionAndDeposit{value: gas}(sessionKey, _expiry(), gas);

        assertEq(game.withdrawable(alice), 0);
        vm.prank(sessionKey);
        vm.expectRevert(CoordinationGameV4.BadStake.selector);
        game.createGame(gameId, mc, _opSig(gameId, mc, alice), alice);
    }

    /// The session authorization itself must be identical to openSession's —
    /// this is the half that is factored out and shared.
    function test_theSessionOpenedThisWayCanActForThePlayer() public {
        vm.prank(alice);
        game.openSessionAndDeposit{value: STAKE}(sessionKey, _expiry(), 0);

        (address key, uint64 exp) = game.sessions(alice);
        assertEq(key, sessionKey, "session key registered");
        assertGt(exp, block.timestamp, "session unexpired");
        assertEq(sessionKey.balance, 0, "gasAmount 0 forwards nothing");
        assertEq(game.withdrawable(alice), STAKE, "all of msg.value was escrowed");
    }

    /// A rejected session key must not swallow the player's stake: the credit is
    /// written before the forward, so the whole call reverts and nothing moves.
    function test_aRevertingSessionKeyStrandsNothing() public {
        RejectsEther hostile = new RejectsEther();
        vm.prank(alice);
        vm.expectRevert(bytes("gas fund failed"));
        game.openSessionAndDeposit{value: 0.0003 ether + STAKE}(address(hostile), _expiry(), 0.0003 ether);

        assertEq(game.withdrawable(alice), 0, "no partial credit survived the revert");
        (address key,) = game.sessions(alice);
        assertEq(key, address(0), "no session was registered");
    }

    // ----- the inline path must survive the upgrade ------------------------

    /// DUAL-MODE is the point: this is a live contract, so a client that still
    /// sends the stake inline has to keep working across the upgrade. Without
    /// this the frontend and the contract would have to ship in lockstep.
    function test_inlineValuePathStillWorksUnchanged() public {
        bytes32 gameId = keccak256("g-inline");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.createGame{value: STAKE}(gameId, mc, _opSig(gameId, mc, alice), alice);

        vm.prank(bob);
        game.joinGame{value: STAKE}(gameId, bob);

        (CoordinationGameV4.Status status,,,) = _game(gameId);
        assertEq(uint8(status), uint8(CoordinationGameV4.Status.Active), "inline path still reaches Active");
        assertEq(game.withdrawable(alice), 0, "inline path does not touch the ledger");
        assertEq(game.withdrawable(bob), 0);
    }

    // ----- the balance is not a shared pot --------------------------------

    /// A player can only ever stake from their OWN balance. `player` is the
    /// debited account, so a session key acting for a wallet spends that
    /// wallet's escrow — never the caller's, and never a third party's.
    function test_oneWalletCannotStakeFromAnothersBalance() public {
        bytes32 gameId = keccak256("g-theft");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.deposit{value: 1 ether}();

        // Mallory tries to open a game "as mallory" with an empty balance while
        // alice's balance is full. It must revert, not reach for alice's funds.
        vm.prank(mallory);
        vm.expectRevert(CoordinationGameV4.BadStake.selector);
        game.createGame(gameId, mc, _opSig(gameId, mc, mallory), mallory);

        assertEq(game.withdrawable(alice), 1 ether, "alice's escrow is untouched");
    }

    /// Staking as someone else still requires acting for them — the escrow path
    /// must not become a way around the session check.
    function test_stakingAsAnotherPlayerStillRequiresAuthorization() public {
        bytes32 gameId = keccak256("g-unauth");
        bytes32 mc = _commitment(gameId);

        vm.prank(alice);
        game.deposit{value: 1 ether}();

        vm.prank(mallory);
        vm.expectRevert(CoordinationGameV4.BadSession.selector);
        game.createGame(gameId, mc, _opSig(gameId, mc, alice), alice);

        assertEq(game.withdrawable(alice), 1 ether, "alice's escrow is untouched");
    }

    /// Destructure the public `games` getter. Kept in one place so a struct
    /// change breaks compilation here rather than silently shifting a field.
    function _game(bytes32 gameId)
        internal
        view
        returns (CoordinationGameV4.Status status, address p1, address p2, uint128 stakeWei)
    {
        (status, p1, p2,,,,, stakeWei,,,,,,,,,) = game.games(gameId);
    }
}

/// A session key that refuses ETH — proves the gas forward is the only failure
/// path and that it takes the whole transaction with it.
contract RejectsEther {
    receive() external payable {
        revert("no");
    }
}
