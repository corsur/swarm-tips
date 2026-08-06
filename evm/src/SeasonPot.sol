// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title SeasonPot — seasons, per-player records, and merkle prize claims
/// @notice The EVM half of the tournament that Solana has had all along.
///
///         Solana splits forfeits into `Tournament.prize_lamports`, publishes a
///         merkle root at `finalize_tournament`, and lets players claim
///         permissionlessly with `claim_reward(amount, proof)`. It works and it
///         has real users — mainnet T1 finalized with 11 eligible players and
///         ~0.73 of 1.375 SOL was actually claimed.
///
///         EVM accrued the identical forfeit share into `prizePoolWei` and
///         offered NO claim path — only `withdrawPrizePool() onlyOwner`. EVM
///         players funded a pot they could never win. This closes that.
///
/// @dev Storage-only base for a UUPS proxy: no constructor, and a reserved gap
///      so the deriving game contract can add state without colliding on an
///      upgrade. Every mutating entry point is guarded by the deriving contract.
abstract contract SeasonPot {
    // -----------------------------------------------------------------------
    // Types
    // -----------------------------------------------------------------------

    struct Season {
        uint64 startTime;
        /// Immutable once set, exactly like Solana's `Tournament.end_time`.
        /// A season MUST expire for a payout to become owed — an open-ended
        /// season makes distribution the owner's discretion rather than an
        /// obligation.
        uint64 endTime;
        bool finalized;
        /// Merkle root over keccak256(0x00 ‖ address ‖ uint256 amount) leaves.
        bytes32 root;
        /// Total promised at finalize; can never exceed the unowed balance.
        uint256 prizeWei;
        /// Falls as players claim. Solana's equivalent field does NOT decrement
        /// — mainnet T1 still reports 1.375 SOL while holding 0.643 — and that
        /// defect is deliberately not reproduced here.
        uint256 remainingWei;
    }

    struct PlayerRecord {
        uint64 wins;
        uint64 games;
        bool claimed;
    }

    // -----------------------------------------------------------------------
    // Storage
    // -----------------------------------------------------------------------

    /// Games a player must finish in a season before any entitlement.
    /// Mirrors `chain_core::game::MIN_GAMES_FOR_PAYOUT`.
    uint64 public constant MIN_GAMES_FOR_PAYOUT = 5;

    /// How long unclaimed prize money waits before the owner may sweep it.
    /// Mirrors Solana's `UNCLAIMED_GRACE_SECS`.
    uint64 public constant UNCLAIMED_GRACE_SECS = 90 days;

    /// A season's default length. One year: the 90-day default is what expired
    /// mainnet tournament 2 on 2026-08-06 and took the Solana game down for 31
    /// hours, because `endTime` cannot be extended.
    uint64 public constant SEASON_DURATION_SECS = 365 days;

    mapping(uint256 => Season) public seasons;
    mapping(uint256 => mapping(address => PlayerRecord)) public records;
    /// The season new games are attributed to.
    uint256 public currentSeasonId;

    /// Reserved so the deriving contract can grow without colliding on upgrade.
    uint256[45] private __gapSeasonPot;

    // -----------------------------------------------------------------------
    // Events / errors
    // -----------------------------------------------------------------------

    event SeasonStarted(uint256 indexed seasonId, uint64 startTime, uint64 endTime);
    event SeasonFinalized(uint256 indexed seasonId, bytes32 root, uint256 prizeWei);
    event PrizeClaimed(uint256 indexed seasonId, address indexed player, uint256 amount);
    event UnclaimedSwept(uint256 indexed seasonId, address indexed to, uint256 amount);

    error SeasonExists();
    error SeasonMissing();
    error SeasonStillOpen();
    error SeasonAlreadyFinalized();
    error SeasonNotFinalized();
    error PromiseExceedsBalance();
    error AlreadyClaimed();
    error BelowMinimumGames();
    error BadProof();
    error NothingToClaim();
    error GraceNotElapsed();

    // -----------------------------------------------------------------------
    // Season lifecycle
    // -----------------------------------------------------------------------

    /// @dev Startable while the current season is still running. Solana's
    ///      rollover hazard is not the expiry itself — it is having no NEXT
    ///      season ready when the current one ends, which is precisely what
    ///      broke mainnet on 2026-08-06.
    function _startSeason(uint256 seasonId) internal {
        if (seasons[seasonId].startTime != 0) revert SeasonExists();
        // forge-lint: disable-next-line(unsafe-typecast)
        // uint64 seconds overflows in year ~584 billion.
        uint64 nowTs = uint64(block.timestamp);
        uint64 end = nowTs + SEASON_DURATION_SECS;
        seasons[seasonId] =
            Season({startTime: nowTs, endTime: end, finalized: false, root: 0, prizeWei: 0, remainingWei: 0});
        currentSeasonId = seasonId;
        emit SeasonStarted(seasonId, nowTs, end);
    }

    /// @dev Requires the season to have ENDED, mirroring
    ///      `finalize_tournament`'s `now > end_time`. `totalWei` is checked
    ///      against what is actually unowed, so a season can never promise more
    ///      than the contract holds.
    function _finalizeSeason(uint256 seasonId, bytes32 root, uint256 totalWei, uint256 alreadyOwed) internal {
        Season storage s = seasons[seasonId];
        if (s.startTime == 0) revert SeasonMissing();
        if (s.finalized) revert SeasonAlreadyFinalized();
        // forge-lint: disable-next-line(block-timestamp)
        // Season windows are YEAR-scale; miner timestamp drift of seconds
        // cannot change whether a season has ended. Mirrors Solana's
        // `finalize_tournament` requiring `now > end_time`.
        if (block.timestamp <= s.endTime) revert SeasonStillOpen();
        if (totalWei + alreadyOwed > address(this).balance) revert PromiseExceedsBalance();

        s.finalized = true;
        s.root = root;
        s.prizeWei = totalWei;
        s.remainingWei = totalWei;
        emit SeasonFinalized(seasonId, root, totalWei);
    }

    // -----------------------------------------------------------------------
    // Player record
    // -----------------------------------------------------------------------

    /// @dev Called on every resolution. `p1Won`/`p2Won` come from the shared
    ///      core's `outcome_to_wins` and are NOT derivable from the amounts:
    ///      HOMOG_BOTH_CORRECT returns each player's own stake (zero net gain)
    ///      and yet awards BOTH a win, because a win records a correct read of
    ///      the opponent rather than a profit.
    function _recordResult(address p1, address p2, bool p1Won, bool p2Won) internal {
        uint256 sid = currentSeasonId;
        PlayerRecord storage r1 = records[sid][p1];
        PlayerRecord storage r2 = records[sid][p2];
        unchecked {
            // u64 games cannot realistically overflow; the pot is bounded by
            // the contract balance long before this does.
            r1.games += 1;
            r2.games += 1;
            if (p1Won) r1.wins += 1;
            if (p2Won) r2.wins += 1;
        }
    }

    // -----------------------------------------------------------------------
    // Claim
    // -----------------------------------------------------------------------

    /// @dev Leaf and node hashing mirror `claim_reward.rs` exactly:
    ///        leaf     = keccak256(0x00 ‖ addr ‖ amount)
    ///        internal = keccak256(0x01 ‖ min ‖ max)
    ///      Domain separation blocks second-preimage attacks; sorted children
    ///      make proofs order-independent, which is what OpenZeppelin's
    ///      `processProof` does natively — so the two agree by construction.
    ///
    ///      Returns the amount owed; the caller performs the transfer so this
    ///      base stays free of value movement (CEI is the caller's to enforce).
    function _claim(uint256 seasonId, address player, uint256 amount, bytes32[] calldata proof)
        internal
        returns (uint256)
    {
        Season storage s = seasons[seasonId];
        if (!s.finalized) revert SeasonNotFinalized();
        if (amount == 0) revert NothingToClaim();

        PlayerRecord storage rec = records[seasonId][player];
        if (rec.claimed) revert AlreadyClaimed();
        if (rec.games < MIN_GAMES_FOR_PAYOUT) revert BelowMinimumGames();

        if (proof.length > MAX_PROOF_LEN) revert BadProof();
        bytes32 leaf = leafFor(player, amount);
        if (_verifyProof(leaf, proof, s.root) == false) revert BadProof();

        // Effects before the caller's interaction.
        rec.claimed = true;
        if (amount > s.remainingWei) revert PromiseExceedsBalance();
        s.remainingWei -= amount;

        emit PrizeClaimed(seasonId, player, amount);
        return amount;
    }

    /// @dev Only after the grace window, and only what is still unclaimed.
    function _sweepUnclaimed(uint256 seasonId) internal returns (uint256) {
        Season storage s = seasons[seasonId];
        if (!s.finalized) revert SeasonNotFinalized();
        // forge-lint: disable-next-line(block-timestamp)
        // 90-day grace; seconds of drift are immaterial.
        if (block.timestamp <= s.endTime + UNCLAIMED_GRACE_SECS) revert GraceNotElapsed();
        uint256 amount = s.remainingWei;
        if (amount == 0) revert NothingToClaim();
        s.remainingWei = 0;
        return amount;
    }

    /// Maximum proof depth — mirrors `claim_reward.rs`'s MAX_PROOF_LEN (2^20
    /// leaves). Bounds gas and matches the Solana bound exactly.
    uint256 internal constant MAX_PROOF_LEN = 20;

    /// @dev Walks the proof with SOLANA's node format, deliberately NOT
    ///      OpenZeppelin's.
    ///
    ///      OZ `MerkleProof.verify` hashes sorted pairs as
    ///      `keccak256(min ‖ max)` with NO domain-separation byte.
    ///      `claim_reward.rs` uses `keccak256(0x01 ‖ min ‖ max)`. Those are
    ///      DIFFERENT trees. Using the library here would silently reject every
    ///      proof produced by the Solana-format finalizer — caught by
    ///      `SeasonPot.t.sol`, which builds its root the Solana way.
    ///
    ///      One format, both chains: that is the whole point of the shared core.
    function _verifyProof(bytes32 leaf, bytes32[] calldata proof, bytes32 root) internal pure returns (bool) {
        bytes32 current = leaf;
        for (uint256 i = 0; i < proof.length; i++) {
            bytes32 sib = proof[i];
            (bytes32 lo, bytes32 hi) = current <= sib ? (current, sib) : (sib, current);
            current = keccak256(abi.encodePacked(bytes1(0x01), lo, hi));
        }
        return current == root;
    }

    /// @notice The merkle leaf for a given entitlement — exposed so the
    ///         off-chain finalizer can be checked against the chain rather than
    ///         trusted to agree with it.
    function leafFor(address player, uint256 amount) public pure returns (bytes32) {
        return keccak256(abi.encodePacked(bytes1(0x00), player, amount));
    }
}
