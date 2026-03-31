use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::WorkSubmitted;
use crate::state::{AgentState, GlobalState, Task, TaskState};
use crate::MAX_CONTENT_ID_LENGTH;

/// Agent submits proof of work (content ID hash).
/// Must be called before deadline minus submit_margin.
/// Decrements the agent's concurrent claim count.
pub fn submit_work(ctx: Context<SubmitWork>, content_id: Vec<u8>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;

    // Checks: protocol not paused
    require!(
        !ctx.accounts.global_state.paused,
        ShillbotError::ProtocolPaused
    );

    // Checks: content_id length bound (instruction input validation)
    require!(
        content_id.len() <= MAX_CONTENT_ID_LENGTH,
        ShillbotError::ContentIdTooLong
    );

    // Checks: state
    require!(
        task.state == TaskState::Claimed,
        ShillbotError::InvalidTaskState
    );

    // Checks: agent identity
    require!(
        ctx.accounts.agent.key() == task.agent,
        ShillbotError::NotTaskAgent
    );

    // Checks: submission before deadline minus margin
    let submission_deadline = task
        .deadline
        .checked_sub(task.submit_margin)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        clock.unix_timestamp < submission_deadline,
        ShillbotError::SubmitMarginInsufficient
    );

    // Checks: agent_state belongs to this agent
    require!(
        ctx.accounts.agent_state.agent == ctx.accounts.agent.key(),
        ShillbotError::NotTaskAgent
    );

    // Checks: claimed_count > 0 (postcondition of claim_task)
    require!(
        ctx.accounts.agent_state.claimed_count > 0,
        ShillbotError::ArithmeticOverflow
    );

    // Compute content ID hash
    let content_id_hash: [u8; 32] = solana_sha256_hasher::hash(&content_id).to_bytes();

    // Effects: decrement agent's concurrent claim count
    let agent_state = &mut ctx.accounts.agent_state;
    agent_state.claimed_count = agent_state
        .claimed_count
        .checked_sub(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Effects: update task
    let task = &mut ctx.accounts.task;
    task.content_id_hash = content_id_hash;
    task.submitted_at = clock.unix_timestamp;
    task.state = TaskState::Submitted;

    // Interactions: none
    emit!(WorkSubmitted {
        task_id: task.task_id,
        agent: ctx.accounts.agent.key(),
        content_id_hash,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct SubmitWork<'info> {
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
    #[account(
        mut,
        seeds = [b"agent_state", agent.key().as_ref()],
        bump = agent_state.bump,
    )]
    pub agent_state: Account<'info, AgentState>,
    pub agent: Signer<'info>,
}
