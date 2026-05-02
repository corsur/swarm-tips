use anchor_lang::prelude::*;

/// Per-agent PDA that tracks the number of currently claimed (but not yet submitted)
/// tasks. Used to enforce the concurrent claim limit on-chain without relying on
/// remaining_accounts, which callers can omit to bypass the check.
///
/// Seeds: `["agent_state", agent_pubkey]`
///
/// **Phase 1 reputation counters** (added 2026-05-02 from previously reserved
/// space): `total_score_sum`, `total_tasks_claimed`, and `total_challenges_lost`
/// are the on-chain inputs to the `agent_profile` MCP tool's derived metrics:
///
///   average_score    = total_score_sum / total_completed
///   completion_rate  = total_completed / total_tasks_claimed
///   dispute_rate     = total_challenges_lost / total_completed
///
/// **Migration safety:** the new fields were carved out of the front of the
/// previous `_reserved: [u8; 32]` block. Existing on-chain accounts had those
/// 24 bytes zero-initialized, so reading them as `u64`s yields 0 — the correct
/// initial value for each counter. Total `SPACE` is unchanged at 90 bytes, so
/// no reallocation is required.
///
/// **Counter semantics:**
/// - `total_completed` and `total_score_sum` increment only when a task
///   finalizes with `payment_amount > 0` (i.e., score >= quality_threshold).
///   This preserves the existing pre-#12 `total_completed` behavior — a
///   below-threshold finalize is a completed task but doesn't count toward
///   the agent's reputation metrics.
/// - `total_tasks_claimed` increments on every `claim_task` call, regardless
///   of the eventual outcome (expire, submit, finalize, dispute). The
///   completion_rate denominator therefore captures "of all tasks they ever
///   started, what fraction did they finish above threshold."
/// - `total_challenges_lost` increments only on `resolve_challenge` with
///   `challenger_won == true`. Updates require the caller to pass AgentState
///   as a remaining_account (same optional pattern as `finalize_task` —
///   if omitted, the counter doesn't update but the resolution still runs).
#[account]
pub struct AgentState {
    /// The agent this state belongs to.
    pub agent: Pubkey,
    /// Number of tasks currently in Claimed state for this agent.
    pub claimed_count: u8,
    /// Total number of tasks this agent has completed (finalized or resolved in their favor).
    pub total_completed: u64,
    /// Total lamports earned by this agent across all tasks.
    pub total_earned: u64,
    /// Running sum of composite scores from above-threshold finalized tasks.
    /// Combined with `total_completed` to derive `average_score` off-chain.
    pub total_score_sum: u64,
    /// Total `claim_task` calls (numerator's denominator for `completion_rate`).
    pub total_tasks_claimed: u64,
    /// Total challenges this agent has lost (basis for `dispute_rate`).
    pub total_challenges_lost: u64,
    /// Reserved space for future fields without reallocation. Reduced from
    /// 32 to 8 bytes when the three counters above were claimed.
    pub _reserved: [u8; 8],
    pub bump: u8,
}

impl AgentState {
    // 8 + 32 + 1 + 8 + 8 + 8 + 8 + 8 + 8 + 1 = 90
    pub const SPACE: usize = 8   // discriminator
        + 32   // agent
        + 1    // claimed_count
        + 8    // total_completed
        + 8    // total_earned
        + 8    // total_score_sum
        + 8    // total_tasks_claimed
        + 8    // total_challenges_lost
        + 8    // _reserved
        + 1; // bump
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::Discriminator;

    #[test]
    fn agent_state_space_is_90() {
        // Total account size is unchanged from before task #12 — the new
        // counter fields were carved out of `_reserved` (32 → 8 bytes).
        // Existing on-chain accounts retain bytewise compatibility because
        // their zero-initialized reserved bytes deserialize as zero counters.
        assert_eq!(AgentState::SPACE, 90);
    }

    /// Migration regression guard for task #12.
    ///
    /// Constructs a byte sequence representing an OLD-shape on-chain account
    /// (pre-#12 layout: `agent | claimed_count | total_completed |
    /// total_earned | _reserved[32] | bump`) and deserializes it as the
    /// NEW struct. Asserts that:
    ///
    ///   - The two pre-existing counters (`total_completed`, `total_earned`)
    ///     still read their original values.
    ///   - The three new counters (`total_score_sum`, `total_tasks_claimed`,
    ///     `total_challenges_lost`) read as 0, because the old `_reserved`
    ///     bytes that now occupy their positions were zero-initialized.
    ///   - The `bump` byte is at the same offset in both layouts and reads
    ///     correctly.
    ///
    /// This catches a future contributor reordering fields. `agent_state_space_is_90`
    /// only asserts SIZE preservation; this test asserts BYTE-LAYOUT
    /// preservation, which is the actual mainnet-compat property.
    #[test]
    fn old_shape_account_deserializes_with_zero_new_counters() {
        let agent_pubkey = Pubkey::new_unique();
        let claimed_count: u8 = 3;
        let total_completed: u64 = 7;
        let total_earned: u64 = 1_000_000;
        let bump: u8 = 254;

        // Build pre-#12 on-chain bytes manually:
        //   discriminator (8) | agent (32) | claimed_count (1) |
        //   total_completed (8) | total_earned (8) | _reserved[32] |
        //   bump (1) = 90 bytes
        let mut bytes = Vec::with_capacity(AgentState::SPACE);
        bytes.extend_from_slice(AgentState::DISCRIMINATOR);
        bytes.extend_from_slice(agent_pubkey.as_ref());
        bytes.push(claimed_count);
        bytes.extend_from_slice(&total_completed.to_le_bytes());
        bytes.extend_from_slice(&total_earned.to_le_bytes());
        // 32 zero bytes — the OLD `_reserved` block. The first 24 bytes
        // become `total_score_sum | total_tasks_claimed |
        // total_challenges_lost` under the new layout (each u64 = 0
        // because those bytes are zero). The last 8 bytes become the
        // new `_reserved`.
        bytes.extend_from_slice(&[0u8; 32]);
        bytes.push(bump);

        assert_eq!(bytes.len(), AgentState::SPACE);

        // Deserialize via Anchor's typed reader (skips discriminator after
        // verifying it matches AgentState::DISCRIMINATOR).
        let parsed = AgentState::try_deserialize(&mut &bytes[..])
            .expect("old-shape bytes must deserialize cleanly under the new layout");

        // Pre-existing fields retain their values bytewise.
        assert_eq!(parsed.agent, agent_pubkey);
        assert_eq!(parsed.claimed_count, claimed_count);
        assert_eq!(parsed.total_completed, total_completed);
        assert_eq!(parsed.total_earned, total_earned);

        // The three new counters MUST read as 0 — they occupy bytes that
        // were zero in the old layout.
        assert_eq!(
            parsed.total_score_sum, 0,
            "total_score_sum must read as 0 from old-shape account"
        );
        assert_eq!(
            parsed.total_tasks_claimed, 0,
            "total_tasks_claimed must read as 0 from old-shape account"
        );
        assert_eq!(
            parsed.total_challenges_lost, 0,
            "total_challenges_lost must read as 0 from old-shape account"
        );

        // `_reserved` shrunk to 8 bytes (also zero from old layout).
        assert_eq!(parsed._reserved, [0u8; 8]);

        // `bump` at the same offset in both layouts.
        assert_eq!(parsed.bump, bump);
    }
}
