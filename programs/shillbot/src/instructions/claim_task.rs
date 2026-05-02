use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskClaimed;
use crate::state::{AgentState, GlobalState, Task, TaskState};

/// Agent claims an open task. Enforces minimum time buffer and concurrent claim limit.
///
/// The concurrent claim limit is enforced via the `AgentState` PDA, which tracks
/// how many tasks the agent currently has in Claimed state. This is tamper-proof
/// because the count is maintained on-chain by claim_task, submit_work, and expire_task.
pub fn claim_task(ctx: Context<ClaimTask>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let agent_state = &ctx.accounts.agent_state;
    let global = &ctx.accounts.global_state;

    // Checks: protocol not paused
    require!(!global.paused, ShillbotError::ProtocolPaused);

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

    // Checks: concurrent claim limit via AgentState counter (read from GlobalState)
    require!(
        agent_state.claimed_count < global.max_concurrent_claims,
        ShillbotError::MaxConcurrentClaimsExceeded
    );

    // Effects: update agent state
    let agent_state = &mut ctx.accounts.agent_state;
    let agent_key = ctx.accounts.agent.key();
    // Only set agent and bump on first initialization (freshly zeroed account)
    if agent_state.agent == Pubkey::default() {
        agent_state.agent = agent_key;
        agent_state.bump = ctx.bumps.agent_state;
    }
    agent_state.claimed_count = agent_state
        .claimed_count
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    // Phase 1 reputation: lifetime claim counter for completion_rate.
    agent_state.total_tasks_claimed = agent_state
        .total_tasks_claimed
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

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
    #[account(
        mut,
        seeds = [
            b"task",
            task.task_id.to_le_bytes().as_ref(),
            task.client.as_ref(),
        ],
        bump = task.bump,
    )]
    pub task: Account<'info, Task>,
    #[account(
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
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
