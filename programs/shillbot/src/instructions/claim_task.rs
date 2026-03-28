use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskClaimed;
use crate::state::{AgentState, Task, TaskState};
use crate::MAX_CONCURRENT_CLAIMS;

/// Agent claims an open task. Enforces minimum time buffer and concurrent claim limit.
///
/// The concurrent claim limit is enforced via the `AgentState` PDA, which tracks
/// how many tasks the agent currently has in Claimed state. This is tamper-proof
/// because the count is maintained on-chain by claim_task, submit_work, and expire_task.
pub fn claim_task(ctx: Context<ClaimTask>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let agent_state = &ctx.accounts.agent_state;

    // Checks: state
    require!(
        task.state == TaskState::Open,
        ShillbotError::InvalidTaskState
    );

    // Checks: minimum time buffer before deadline
    let earliest_claim_deadline = clock
        .unix_timestamp
        .checked_add(task.claim_buffer)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        earliest_claim_deadline < task.deadline,
        ShillbotError::ClaimBufferInsufficient
    );

    // Checks: concurrent claim limit via AgentState counter
    require!(
        agent_state.claimed_count < MAX_CONCURRENT_CLAIMS,
        ShillbotError::MaxConcurrentClaimsExceeded
    );

    // Effects: update agent state
    let agent_state = &mut ctx.accounts.agent_state;
    let agent_key = ctx.accounts.agent.key();
    agent_state.agent = agent_key;
    agent_state.claimed_count = agent_state
        .claimed_count
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    agent_state.bump = ctx.bumps.agent_state;

    // Effects: update task
    let task = &mut ctx.accounts.task;
    task.agent = agent_key;
    task.state = TaskState::Claimed;

    // Interactions: none
    emit!(TaskClaimed {
        task_id: task.task_id,
        agent: agent_key,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct ClaimTask<'info> {
    #[account(mut)]
    pub task: Account<'info, Task>,
    /// AgentState PDA tracks the agent's concurrent claim count.
    ///
    /// Using `init_if_needed` is acceptable here because:
    /// (a) the agent pays for creation — no cost to the protocol,
    /// (b) AgentState does not hold escrow funds,
    /// (c) the "no init_if_needed for escrow accounts" rule does not apply.
    /// The account is initialized on the agent's first claim and reused thereafter.
    #[account(
        init_if_needed,
        payer = agent,
        space = AgentState::SPACE,
        seeds = [b"agent_state", agent.key().as_ref()],
        bump,
    )]
    pub agent_state: Account<'info, AgentState>,
    #[account(mut)]
    pub agent: Signer<'info>,
    pub system_program: Program<'info, System>,
}
