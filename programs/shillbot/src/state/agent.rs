use anchor_lang::prelude::*;

/// Per-agent PDA that tracks the number of currently claimed (but not yet submitted)
/// tasks. Used to enforce the concurrent claim limit on-chain without relying on
/// remaining_accounts, which callers can omit to bypass the check.
///
/// Seeds: `["agent_state", agent_pubkey]`
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
    /// Reserved space for future fields without reallocation.
    pub _reserved: [u8; 32],
    pub bump: u8,
}

impl AgentState {
    // 8 + 32 + 1 + 8 + 8 + 32 + 1 = 90
    pub const SPACE: usize = 8   // discriminator
        + 32   // agent
        + 1    // claimed_count
        + 8    // total_completed
        + 8    // total_earned
        + 32   // _reserved
        + 1; // bump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_state_space_is_90() {
        assert_eq!(AgentState::SPACE, 90);
    }
}
