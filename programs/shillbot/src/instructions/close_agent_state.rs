use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::AgentStateClosed;
use crate::state::AgentState;

/// Allows an agent to close their AgentState PDA and reclaim rent (~0.001 SOL).
/// Only permitted when the agent has no active claims (`claimed_count == 0`),
/// no lost challenges (`total_challenges_lost == 0`), and no active strike
/// cooldown (`T1-07`), preventing reputation-wipe and cooldown-bypass cycles.
pub fn close_agent_state(ctx: Context<CloseAgentState>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    validate_close_agent_state(&ctx.accounts.agent_state, &ctx.accounts.agent.key(), now)?;

    // Effects: account closed by Anchor `close` constraint

    // Interactions: emit event
    emit!(AgentStateClosed {
        agent: ctx.accounts.agent.key(),
    });

    Ok(())
}

pub(crate) fn validate_close_agent_state(
    agent_state: &AgentState,
    agent_key: &Pubkey,
    now: i64,
) -> Result<()> {
    // Checks: agent owns this state
    require!(agent_state.agent == *agent_key, ShillbotError::NotTaskAgent);

    // Checks: no active claims
    require!(
        agent_state.claimed_count == 0,
        ShillbotError::AgentStateHasActiveClaims
    );

    // Checks: cannot close AgentState to erase lost challenges (SC-MED-04)
    require!(
        agent_state.total_challenges_lost == 0,
        ShillbotError::InvalidTaskState
    );

    // Checks: cannot close AgentState to bypass an active score-0 strike cooldown (T1-07)
    crate::instructions::claim_task::validate_strike_cooldown(agent_state, now)?;

    Ok(())
}

#[derive(Accounts)]
pub struct CloseAgentState<'info> {
    #[account(
        mut,
        close = agent,
        seeds = [b"agent_state", agent.key().as_ref()],
        bump = agent_state.bump,
    )]
    pub agent_state: Account<'info, AgentState>,
    #[account(mut)]
    pub agent: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_agent_state(agent: Pubkey) -> AgentState {
        AgentState {
            agent,
            claimed_count: 0,
            total_completed: 1,
            total_earned: 1_000_000,
            total_score_sum: 1_000_000,
            total_tasks_claimed: 1,
            total_challenges_lost: 0,
            last_strike_at: 0,
            bump: 255,
        }
    }

    #[test]
    fn close_agent_state_blocks_active_strike_cooldown_and_allows_after_expiry() {
        let agent = Pubkey::new_unique();
        let mut st = sample_agent_state(agent);
        assert!(validate_close_agent_state(&st, &agent, 1_000).is_ok());

        // Struck at t = 1000: closing during the 60s cooldown must revert (T1-07).
        st.last_strike_at = 1_000;
        assert!(validate_close_agent_state(&st, &agent, 1_000).is_err());
        assert!(validate_close_agent_state(
            &st,
            &agent,
            1_000 + crate::constants::SCORE_ZERO_COOLDOWN_SECONDS - 1
        )
        .is_err());

        // Once the cooldown expires, closing succeeds.
        assert!(validate_close_agent_state(
            &st,
            &agent,
            1_000 + crate::constants::SCORE_ZERO_COOLDOWN_SECONDS
        )
        .is_ok());
    }
}
