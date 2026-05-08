use anchor_lang::prelude::*;

#[account]
pub struct GlobalState {
    /// Monotonic counter incremented on each create_task.
    pub task_counter: u64,
    /// Single upgrade authority key (EOA on both devnet and mainnet for v1).
    pub authority: Pubkey,
    /// Treasury account for protocol fee collection.
    pub treasury: Pubkey,
    /// Protocol fee in basis points (100 = 1%).
    pub protocol_fee_bps: u16,
    /// Minimum composite score for payment (fixed-point, max = MAX_SCORE).
    pub quality_threshold: u64,
    /// Duration of the challenge window after verification (seconds).
    pub challenge_window_seconds: i64,
    /// Maximum time allowed for oracle verification after submission (seconds).
    pub verification_timeout_seconds: i64,
    /// Minimum delay between submission and attestation (seconds).
    pub attestation_delay_seconds: i64,
    /// Maximum age of oracle data relative to submission (seconds).
    pub staleness_window_seconds: i64,
    /// Maximum number of tasks an agent can have in Claimed state simultaneously.
    pub max_concurrent_claims: u8,
    /// Challenge bond multiplier — raw u8 multiplier on the task's escrow,
    /// stored in a u16 slot for forward compat.
    ///
    /// **Naming caveat:** the `_bps` suffix is misleading. Despite the
    /// name, this is **NOT** basis points (1bp = 0.01%). It's a raw
    /// multiplier: `2` means 2x escrow, `10` means 10x escrow, NOT
    /// `200` for 2% or `20000` for 200%. The wire format is u16 only
    /// because the original design considered bps semantics; the
    /// final spec uses `MIN_CHALLENGE_BOND_MULTIPLIER..=MAX` which
    /// are u8 values [2, 10]. `update_params` accepts `u8` and casts
    /// to u16 on write. `challenge_task` reads this and multiplies
    /// against `task.escrow_lamports` directly. The field is NOT
    /// renamed because that's a wire-format change requiring an
    /// account migration; the `_bps` suffix stays for binary compat.
    pub challenge_bond_multiplier_bps: u16,
    /// Portion of slashed bond sent to treasury in basis points.
    pub bond_slash_treasury_bps: u16,
    /// Authority allowed to submit oracle attestations.
    pub oracle_authority: Pubkey,
    /// Whether the entire protocol is paused.
    pub paused: bool,
    /// Bitmask of paused platforms (bit N = PlatformType with value N).
    pub paused_platforms: u16,
    /// Switchboard pull feed account that provides oracle-attested
    /// composite scores. Read by `verify_task` to validate the feed
    /// passed in by the caller. Different per-network (mainnet
    /// `En9CN…T6qQ`, devnet `BSSv19i…`); rotated via the multisig if
    /// needed (re-adding `set_switchboard_feed` is a follow-up). The
    /// 2026-05-01 lock-down to a compile-time const was reverted on
    /// 2026-05-08 because the const shipped as a placeholder and the
    /// load-bearing TODO was missed — see `verify_task.rs` comment.
    pub switchboard_feed: Pubkey,
    /// Minimum task escrow in lamports (Phase 3 blocker #2 — D2).
    /// Carved from `_reserved` 2026-05-07. Pre-D2 was a compile-time
    /// **Vestigial slot — retired 2026-05-07.** Was the per-task
    /// minimum escrow floor for sybil deterrence (Phase 3 blocker #2).
    /// Removed because the friction on legitimate small clients
    /// exceeded the benefit at solo + pre-PMF scale. The right
    /// end-state defense is the EigenTrust reputation graph (Phase 2
    /// in `swarm-tips/CLAUDE.md`). The slot is preserved here so
    /// existing on-chain 227-byte GlobalState accounts continue to
    /// deserialize unchanged; no instruction reads or writes it.
    /// `initialize` zero-fills it; future GlobalState v2 may rename
    /// to `_reserved_min_escrow`.
    pub min_escrow_lamports: u64,
    /// Per-client task-creation rate-limit window length (D3).
    /// Same migration story as `min_escrow_lamports`: was const,
    /// now governance with bounds.
    pub rate_limit_window_seconds: i64,
    /// Per-client task-creation rate-limit count within the window
    /// (D3). Same migration story.
    pub max_tasks_per_rate_window: u32,
    /// Reserved space for future fields without reallocation.
    /// Reduced from 32 → 19 bytes by D2/D3 (carved 8 + 8 + 4 = 20
    /// bytes; net loss is the 1 byte alignment between the new
    /// fields and bump, conservatively rounded).
    pub _reserved: [u8; 12],
    pub bump: u8,
}

impl GlobalState {
    pub const SPACE: usize = 8   // discriminator
        + 8    // task_counter
        + 32   // authority
        + 32   // treasury
        + 2    // protocol_fee_bps
        + 8    // quality_threshold
        + 8    // challenge_window_seconds
        + 8    // verification_timeout_seconds
        + 8    // attestation_delay_seconds
        + 8    // staleness_window_seconds
        + 1    // max_concurrent_claims
        + 2    // challenge_bond_multiplier_bps
        + 2    // bond_slash_treasury_bps
        + 32   // oracle_authority
        + 1    // paused
        + 2    // paused_platforms
        + 32   // switchboard_feed
        + 8    // min_escrow_lamports         (D2 — was const)
        + 8    // rate_limit_window_seconds   (D3 — was const)
        + 4    // max_tasks_per_rate_window   (D3 — was const)
        + 12   // _reserved (was 32, carved 20 by D2+D3)
        + 1; // bump
             // Total = 227 (unchanged); bytewise-compatible with v4 layout
             // because the carved bytes were zero in `_reserved`.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_state_space_is_227() {
        assert_eq!(GlobalState::SPACE, 227);
    }

    /// Migration regression guard for D2/D3.
    ///
    /// Builds a byte sequence representing the OLD layout (pre-D2/D3:
    /// 32-byte `_reserved` block where the new fields now live) and
    /// deserializes it as the NEW struct. Asserts that:
    ///   - All pre-existing fields keep their values
    ///   - The 3 new fields (`min_escrow_lamports`,
    ///     `rate_limit_window_seconds`, `max_tasks_per_rate_window`)
    ///     read as 0 from the zero-initialized old `_reserved` bytes
    ///   - `bump` is at the correct offset in both layouts
    ///
    /// Pre-D2/D3 callers must `update_params` to set non-zero values
    /// for the 3 new fields after the program upgrade lands; until they
    /// do, `create_task` will fail because escrow >= 0 is trivially
    /// satisfied but the rate-limit window of 0 seconds means the
    /// limit is never reached.
    #[test]
    fn old_layout_deserializes_with_zero_new_fields() {
        use anchor_lang::Discriminator;

        let authority = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let oracle = Pubkey::new_unique();
        let feed = Pubkey::new_unique();
        let bump: u8 = 255;

        // Pre-D2/D3 layout: same as current up to switchboard_feed,
        // then 32-byte _reserved, then bump.
        let mut bytes = Vec::with_capacity(GlobalState::SPACE);
        bytes.extend_from_slice(GlobalState::DISCRIMINATOR);
        bytes.extend_from_slice(&123u64.to_le_bytes()); // task_counter
        bytes.extend_from_slice(authority.as_ref());
        bytes.extend_from_slice(treasury.as_ref());
        bytes.extend_from_slice(&100u16.to_le_bytes()); // protocol_fee_bps
        bytes.extend_from_slice(&200_000u64.to_le_bytes()); // quality_threshold
        bytes.extend_from_slice(&86_400i64.to_le_bytes()); // challenge_window_seconds
        bytes.extend_from_slice(&1_209_600i64.to_le_bytes()); // verification_timeout_seconds
        bytes.extend_from_slice(&604_800i64.to_le_bytes()); // attestation_delay_seconds
        bytes.extend_from_slice(&86_400i64.to_le_bytes()); // staleness_window_seconds
        bytes.push(5u8); // max_concurrent_claims
        bytes.extend_from_slice(&20_000u16.to_le_bytes()); // challenge_bond_multiplier_bps
        bytes.extend_from_slice(&5_000u16.to_le_bytes()); // bond_slash_treasury_bps
        bytes.extend_from_slice(oracle.as_ref());
        bytes.push(0u8); // paused = false
        bytes.extend_from_slice(&0u16.to_le_bytes()); // paused_platforms
        bytes.extend_from_slice(feed.as_ref());
        // 32 zero bytes — the OLD `_reserved` block. The first 20
        // bytes become min_escrow_lamports + rate_limit_window_seconds
        // + max_tasks_per_rate_window under the new layout (all u-int
        // = 0 because those bytes are zero). The last 12 bytes become
        // the new `_reserved`.
        bytes.extend_from_slice(&[0u8; 32]);
        bytes.push(bump);

        assert_eq!(bytes.len(), GlobalState::SPACE);

        let parsed = GlobalState::try_deserialize(&mut &bytes[..])
            .expect("old layout must deserialize cleanly under new layout");

        assert_eq!(parsed.task_counter, 123);
        assert_eq!(parsed.authority, authority);
        assert_eq!(parsed.treasury, treasury);
        assert_eq!(parsed.protocol_fee_bps, 100);
        assert_eq!(parsed.quality_threshold, 200_000);
        assert_eq!(parsed.bump, bump);

        // The 3 new fields MUST read as 0 (operator must `update_params`
        // post-upgrade to set non-zero values).
        assert_eq!(parsed.min_escrow_lamports, 0);
        assert_eq!(parsed.rate_limit_window_seconds, 0);
        assert_eq!(parsed.max_tasks_per_rate_window, 0);
        assert_eq!(parsed._reserved, [0u8; 12]);
    }
}
