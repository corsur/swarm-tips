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
    pub bump: u8,
}

impl AgentState {
    pub const SPACE: usize = 8  // discriminator
        + 32  // agent
        + 1   // claimed_count
        + 1; // bump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_state_space_is_42() {
        assert_eq!(AgentState::SPACE, 42);
    }
}
