// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CoordinationGameV4} from "../src/CoordinationGameV4.sol";
import {ERC1967Proxy} from "../lib/openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";

/// @title Storage layout pin for the UUPS proxy
///
/// WHAT THIS CATCHES THAT THE EXISTING UPGRADE TESTS DO NOT
/// -------------------------------------------------------
/// `test_upgradePreservesState` upgrades to a V5 that appends nothing and
/// checks the VALUES survive. That passes no matter where the values live, so
/// it cannot see the actual upgrade hazard here:
///
///   slot 0  _owner             Ownable
///   slot 1  _pendingOwner      Ownable2Step
///   slot 2  withdrawable       PullPayment      <-- NO __gap
///   slot 3  _paused            Pausable         <-- packed with...
///   slot 3  _authorizedSigner  AttesterGated    <-- ...this, NO __gap
///   slot 4  seasons            SeasonPot
///   slot 5  records            SeasonPot
///   slot 6  currentSeasonId    SeasonPot
///   slot 7  __gapSeasonPot[45] -> occupies 7..51
///   slot 52 games              CoordinationGameV4
///
/// SeasonPot is safe to grow: its gap absorbs 45 slots before it reaches
/// `games`. The BASES BELOW IT ARE NOT. Adding one field to PullPayment or
/// AttesterGated — or unpacking slot 3 — shifts SeasonPot and every v4
/// variable down by a slot. On an upgrade that silently reinterprets live
/// storage: `withdrawable` balances read as season records, `games` reads as
/// `sessions`, and staked funds are misattributed rather than lost loudly.
///
/// These asserts are deliberately about SLOT NUMBERS, not values. A test that
/// only reads through getters follows the variable wherever it moved, which is
/// exactly the drift this must fail on.
contract CoordinationGameV4LayoutTest is Test {
    CoordinationGameV4 internal game;
    address internal owner = address(0xA11CE);
    address internal operator = address(0x09E7A);
    address internal treasury = address(0xCAFE);

    // Pinned. Changing one of these constants is a deliberate act that must be
    // paired with a migration plan for every deployed proxy.
    uint256 internal constant SLOT_OWNER = 0;
    uint256 internal constant SLOT_PENDING_OWNER = 1;
    uint256 internal constant SLOT_WITHDRAWABLE = 2;
    uint256 internal constant SLOT_PAUSED_AND_SIGNER = 3;
    uint256 internal constant SLOT_SEASONS = 4;
    uint256 internal constant SLOT_CURRENT_SEASON_ID = 6;
    uint256 internal constant SLOT_GAP_START = 7;
    uint256 internal constant SLOT_GAMES = 52;

    function setUp() public {
        CoordinationGameV4 impl = new CoordinationGameV4();
        bytes memory init = abi.encodeCall(
            CoordinationGameV4.initialize, (owner, operator, treasury, 5000, 0.0027 ether, 3600, 7200, 1, 365 days)
        );
        game = CoordinationGameV4(payable(address(new ERC1967Proxy(address(impl), init))));
    }

    function _slot(uint256 s) internal view returns (bytes32) {
        return vm.load(address(game), bytes32(s));
    }

    function test_ownershipSlotsAreWhereTheProxyExpects() public view {
        assertEq(address(uint160(uint256(_slot(SLOT_OWNER)))), owner, "owner must stay at slot 0");
        assertEq(uint256(_slot(SLOT_PENDING_OWNER)), 0, "pendingOwner must stay at slot 1");
    }

    /// PullPayment holds real money and has NO storage gap beneath it.
    function test_withdrawableMappingIsAnchoredAtItsSlot() public {
        // Credit a player by resolving nothing — poke the mapping directly at
        // the slot we claim it lives at, then read it back through the getter.
        // If the mapping moved, the getter reads a different slot and returns 0.
        address player = address(0xBEEF);
        bytes32 entry = keccak256(abi.encode(player, SLOT_WITHDRAWABLE));
        vm.store(address(game), entry, bytes32(uint256(123_456)));
        assertEq(
            game.withdrawable(player),
            123_456,
            "withdrawable must live at slot 2: PullPayment has no gap, so a field added to it silently moves every balance"
        );
    }

    function test_pausedAndSignerStillShareSlotThree() public view {
        // bool(1) + address(20) = 21 bytes, so they pack. If either base gains
        // a field, or the bool is widened, this unpacks and everything below
        // shifts by a slot.
        bytes32 packed = _slot(SLOT_PAUSED_AND_SIGNER);
        address signer = address(uint160(uint256(packed) >> 8));
        assertEq(signer, operator, "operator must be readable from the high bytes of slot 3");
        assertEq(uint256(packed) & 0xff, 0, "paused flag occupies the low byte and is false");
    }

    function test_seasonPotSlotsAndGapAreIntact() public {
        vm.prank(owner);
        game.startSeason(2, 365 days);

        assertEq(uint256(_slot(SLOT_CURRENT_SEASON_ID)), 2, "currentSeasonId must stay at slot 6");

        // The gap must still be untouched storage — if a real variable had been
        // introduced into it, initialize/startSeason would have written here.
        for (uint256 i = SLOT_GAP_START; i < SLOT_GAP_START + 5; i++) {
            assertEq(uint256(_slot(i)), 0, "SeasonPot gap must remain unwritten");
        }

        // seasons[2].startTime is the first word of the struct at its mapping slot.
        bytes32 seasonEntry = keccak256(abi.encode(uint256(2), SLOT_SEASONS));
        assertGt(uint256(vm.load(address(game), seasonEntry)), 0, "seasons must live at slot 4");
    }

    /// The first v4-owned variable must sit immediately after SeasonPot's gap.
    /// If this moves, SeasonPot's gap was resized or a base grew.
    function test_v4OwnStateStartsRightAfterTheGap() public {
        bytes32 gameId = keccak256("layout-probe");
        bytes32 entry = keccak256(abi.encode(gameId, SLOT_GAMES));
        // Game.status is the first field; write Cancelled(6) and read it back.
        vm.store(address(game), entry, bytes32(uint256(6)));
        (CoordinationGameV4.Status status,,,,,,,,,,,,,,,,) = game.games(gameId);
        assertEq(uint8(status), 6, "games must start at slot 52, immediately after the 45-slot gap");
    }
}
