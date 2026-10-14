use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::AgentStateClosed;
use crate::state::AgentState;

/// Allows an agent to close their AgentState PDA and reclaim rent (~0.001 SOL).
/// Only permitted when the agent has no active claims (`claimed_count == 0`)
/// and no negative reputation record (`total_challenges_lost == 0` and
/// `total_completed == total_tasks_claimed`), preventing reputation-wipe cycles.
pub fn close_agent_state(ctx: Context<CloseAgentState>) -> Result<()> {
    let agent_state = &ctx.accounts.agent_state;

    // Checks: agent owns this state
    require!(
        agent_state.agent == ctx.accounts.agent.key(),
        ShillbotError::NotTaskAgent
    );

    // Checks: no active claims
    require!(
        agent_state.claimed_count == 0,
        ShillbotError::AgentStateHasActiveClaims
    );

    // Checks: cannot close AgentState to erase lost challenges or abandoned claims (SC-MED-04)
    require!(
        agent_state.total_challenges_lost == 0
            && agent_state.total_completed == agent_state.total_tasks_claimed,
        ShillbotError::InvalidTaskState
    );

    // Effects: account closed by Anchor `close` constraint

    // Interactions: emit event
    emit!(AgentStateClosed {
        agent: ctx.accounts.agent.key(),
    });

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
