// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {SeasonPot} from "../src/SeasonPot.sol";

/// Minimal concrete shell — exposes the internals so the base is tested
/// directly rather than through the whole game contract.
contract PotHarness is SeasonPot {
    function startSeason(uint256 id) external {
        _startSeason(id);
    }

    function finalizeSeason(uint256 id, bytes32 root, uint256 total) external {
        _finalizeSeason(id, root, total);
    }

    function accrue(uint256 amount) external {
        _accrue(amount);
    }

    function recordResult(address p1, address p2, bool w1, bool w2) external {
        _recordResult(p1, p2, w1, w2);
    }

    function claim(uint256 id, uint256 amount, bytes32[] calldata proof) external returns (uint256) {
        uint256 owed = _claim(id, msg.sender, amount, proof);
        (bool ok,) = payable(msg.sender).call{value: owed}("");
        require(ok, "pay failed");
        return owed;
    }

    function sweep(uint256 id) external returns (uint256) {
        return _sweepUnclaimed(id);
    }

    receive() external payable {}
}

contract SeasonPotTest is Test {
    PotHarness internal pot;
    address internal alice = address(0xA11CE);
    address internal bob = address(0xB0B);
    address internal carol = address(0xCA401);

    function setUp() public {
        pot = new PotHarness();
        vm.deal(address(pot), 10 ether);
    }

    /// Two-leaf tree so a real proof exists: root = keccak(0x01 ‖ min ‖ max).
    function _tree(bytes32 a, bytes32 b) internal pure returns (bytes32 root) {
        return
            a <= b ? keccak256(abi.encodePacked(bytes1(0x01), a, b)) : keccak256(abi.encodePacked(bytes1(0x01), b, a));
    }

    function _play(address p, uint256 n) internal {
        for (uint256 i = 0; i < n; i++) {
            pot.recordResult(p, address(0xDEAD), true, false);
        }
    }

    // ----- seasons ---------------------------------------------------------

    function test_seasonRunsOneYear_andCannotBeStartedTwice() public {
        pot.startSeason(1);
        (uint64 start, uint64 end,,,,,) = pot.seasons(1);
        assertEq(end - start, 365 days, "a season is a year");
        vm.expectRevert(SeasonPot.SeasonExists.selector);
        pot.startSeason(1);
    }

    /// The rollover hazard is having no NEXT season, not the expiry itself.
    function test_nextSeasonCanStartWhileCurrentIsLive() public {
        pot.startSeason(1);
        pot.startSeason(2);
        assertEq(pot.currentSeasonId(), 2);
    }

    /// A season must EXPIRE for a payout to be owed — otherwise distribution is
    /// the owner's discretion rather than an obligation.
    function test_finalizeRevertsBeforeTheSeasonEnds() public {
        pot.startSeason(1);
        vm.expectRevert(SeasonPot.SeasonStillOpen.selector);
        pot.finalizeSeason(1, bytes32(uint256(1)), 1 ether);
    }

    /// A season may only promise what IT accrued. Bounding on the contract
    /// BALANCE instead would let one season promise another season's money, or
    /// money still owed to unclaimed players elsewhere.
    function test_finalizeCannotPromiseMoreThanTheSeasonAccrued() public {
        pot.startSeason(1);
        pot.accrue(1 ether);
        vm.warp(block.timestamp + 366 days);
        // The contract holds 10 ETH, but THIS season only took in 1.
        vm.expectRevert(SeasonPot.PromiseExceedsBalance.selector);
        pot.finalizeSeason(1, bytes32(uint256(1)), 2 ether);
        // Exactly what it accrued is fine.
        pot.finalizeSeason(1, bytes32(uint256(1)), 1 ether);
    }

    // ----- player record ---------------------------------------------------

    /// Wins are NOT derivable from the amounts. Both-correct pays each player
    /// their own stake back — zero net gain — and awards BOTH a win.
    function test_bothCorrectAwardsTwoWins() public {
        pot.startSeason(1);
        pot.recordResult(alice, bob, true, true);
        (uint64 aw, uint64 ag,) = pot.records(1, alice);
        (uint64 bw, uint64 bg,) = pot.records(1, bob);
        assertEq(aw, 1);
        assertEq(bw, 1);
        assertEq(ag, 1);
        assertEq(bg, 1);
    }

    function test_recordsAreScopedToTheCurrentSeason() public {
        pot.startSeason(1);
        pot.recordResult(alice, bob, true, false);
        pot.startSeason(2);
        (, uint64 gamesS1,) = pot.records(1, alice);
        (, uint64 gamesS2,) = pot.records(2, alice);
        assertEq(gamesS1, 1, "season 1 keeps its record");
        assertEq(gamesS2, 0, "season 2 starts clean");
    }

    // ----- claim -----------------------------------------------------------

    function _finalizedTreeSeason() internal returns (bytes32[] memory proofA, uint256 amtA) {
        pot.startSeason(1);
        _play(alice, 5);
        _play(bob, 5);
        amtA = 1 ether;
        bytes32 leafA = pot.leafFor(alice, amtA);
        bytes32 leafB = pot.leafFor(bob, 2 ether);
        vm.warp(block.timestamp + 366 days);
        pot.accrue(3 ether);
        pot.finalizeSeason(1, _tree(leafA, leafB), 3 ether);
        proofA = new bytes32[](1);
        proofA[0] = leafB;
    }

    function test_claimPaysOnce_andDrawsRemainingDown() public {
        (bytes32[] memory proof, uint256 amt) = _finalizedTreeSeason();
        uint256 before = alice.balance;

        vm.prank(alice);
        pot.claim(1, amt, proof);

        assertEq(alice.balance - before, amt, "paid exactly the entitlement");
        (,,,,, uint256 prize, uint256 remaining) = pot.seasons(1);
        assertEq(prize, 3 ether, "promise is unchanged");
        assertEq(remaining, 2 ether, "remaining FELL by the claim");

        // Solana's field does not decrement — mainnet T1 reports 1.375 SOL
        // while holding 0.643. That defect is not reproduced here.
        assertLt(remaining, prize);

        vm.prank(alice);
        vm.expectRevert(SeasonPot.AlreadyClaimed.selector);
        pot.claim(1, amt, proof);
    }

    function test_wrongProofAndWrongAmountBothRevert() public {
        (bytes32[] memory proof, uint256 amt) = _finalizedTreeSeason();

        // One byte of the proof flipped.
        bytes32[] memory bad = new bytes32[](1);
        bad[0] = bytes32(uint256(proof[0]) ^ 1);
        vm.prank(alice);
        vm.expectRevert(SeasonPot.BadProof.selector);
        pot.claim(1, amt, bad);

        // One wei more than the leaf commits to.
        vm.prank(alice);
        vm.expectRevert(SeasonPot.BadProof.selector);
        pot.claim(1, amt + 1, proof);
    }

    function test_ineligiblePlayerCannotClaim() public {
        pot.startSeason(1);
        _play(carol, 4); // one short of MIN_GAMES_FOR_PAYOUT
        bytes32 leaf = pot.leafFor(carol, 1 ether);
        vm.warp(block.timestamp + 366 days);
        pot.accrue(1 ether);
        pot.finalizeSeason(1, _tree(leaf, leaf), 1 ether);

        bytes32[] memory proof = new bytes32[](1);
        proof[0] = leaf;
        vm.prank(carol);
        vm.expectRevert(SeasonPot.BelowMinimumGames.selector);
        pot.claim(1, 1 ether, proof);
    }

    function test_claimRevertsBeforeFinalize() public {
        pot.startSeason(1);
        _play(alice, 5);
        bytes32[] memory proof = new bytes32[](0);
        vm.prank(alice);
        vm.expectRevert(SeasonPot.SeasonNotFinalized.selector);
        pot.claim(1, 1 ether, proof);
    }

    // ----- sweep -----------------------------------------------------------

    function test_sweepWaitsOutTheGrace_thenTakesOnlyWhatIsUnclaimed() public {
        (bytes32[] memory proof, uint256 amt) = _finalizedTreeSeason();
        vm.prank(alice);
        pot.claim(1, amt, proof);

        vm.expectRevert(SeasonPot.GraceNotElapsed.selector);
        pot.sweep(1);

        vm.warp(block.timestamp + 91 days);
        assertEq(pot.sweep(1), 2 ether, "only the unclaimed remainder");

        vm.expectRevert(SeasonPot.NothingToClaim.selector);
        pot.sweep(1);
    }

    // ----- cross-language merkle format -------------------------------------

    /// The leaf AND node format must match what the Solana-format finalizer
    /// produces, byte for byte.
    ///
    /// This test exists because it already caught a real bug: the first version
    /// of `_claim` used OpenZeppelin's `MerkleProof.verify`, which hashes sorted
    /// pairs as `keccak256(min ‖ max)` with NO domain-separation byte, while
    /// `claim_reward.rs` uses `keccak256(0x01 ‖ min ‖ max)`. Those are different
    /// trees — the library would have rejected every proof the finalizer
    /// produced, and no happy-path test would have noticed because the test
    /// would have built its tree the library's way too.
    function test_merkleFormatMatchesTheSharedFixture() public view {
        string memory json = vm.readFile("../tests/fixtures/game-payout-vectors.json");
        address addrA = vm.parseJsonAddress(json, ".merkle.addrA");
        address addrB = vm.parseJsonAddress(json, ".merkle.addrB");
        uint256 amtA = vm.parseUint(vm.parseJsonString(json, ".merkle.amountA"));
        uint256 amtB = vm.parseUint(vm.parseJsonString(json, ".merkle.amountB"));
        bytes32 wantLeafA = vm.parseJsonBytes32(json, ".merkle.leafA");
        bytes32 wantLeafB = vm.parseJsonBytes32(json, ".merkle.leafB");
        bytes32 wantRoot = vm.parseJsonBytes32(json, ".merkle.root");

        bytes32 gotA = pot.leafFor(addrA, amtA);
        bytes32 gotB = pot.leafFor(addrB, amtB);
        assertEq(gotA, wantLeafA, "leaf A diverges from the shared format");
        assertEq(gotB, wantLeafB, "leaf B diverges from the shared format");
        assertEq(_tree(gotA, gotB), wantRoot, "root diverges: check the 0x01 node byte");

        // And prove the OZ format would NOT have matched, so the distinction is
        // pinned rather than merely commented.
        bytes32 ozStyle =
            gotA <= gotB ? keccak256(abi.encodePacked(gotA, gotB)) : keccak256(abi.encodePacked(gotB, gotA));
        assertTrue(ozStyle != wantRoot, "OZ's node format must NOT equal ours");
    }

    // ----- leaf format -----------------------------------------------------

    /// Must match `claim_reward.rs`: keccak256(0x00 ‖ addr ‖ amount).
    function test_leafFormatMatchesTheSolanaProgram() public view {
        bytes32 expected = keccak256(abi.encodePacked(bytes1(0x00), alice, uint256(1 ether)));
        assertEq(pot.leafFor(alice, 1 ether), expected);
        // Domain separation: a leaf must never collide with an internal node.
        assertTrue(pot.leafFor(alice, 1 ether) != keccak256(abi.encodePacked(bytes1(0x01), alice, uint256(1 ether))));
    }
}
