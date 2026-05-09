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
    let agent_key = ctx.accounts.agent.key();

    // Checks: protocol-pause + content-id bound + state + agent identity +
    // deadline-minus-margin + agent_state agent match + claimed_count > 0.
    validate_submit_common(
        &ctx.accounts.global_state,
        &ctx.accounts.task,
        &agent_key,
        content_id.len(),
        &ctx.accounts.agent_state,
        clock.unix_timestamp,
    )?;

    let content_id_hash: [u8; 32] = solana_sha256_hasher::hash(&content_id).to_bytes();

    // Effects.
    commit_submit_state(
        &mut ctx.accounts.agent_state,
        &mut ctx.accounts.task,
        content_id_hash,
        clock.unix_timestamp,
    )?;

    // Interactions: none
    emit!(WorkSubmitted {
        task_id: ctx.accounts.task.task_id,
        agent: agent_key,
        content_id_hash,
    });

    Ok(())
}

/// Common validation shared between `submit_work` and `submit_work_session`.
/// Pure check phase — caller passes the agent pubkey to match (the wallet
/// variant uses `agent.key()`; the session variant uses `session.agent`).
pub(crate) fn validate_submit_common(
    global: &GlobalState,
    task: &Task,
    expected_agent: &Pubkey,
    content_id_len: usize,
    agent_state: &AgentState,
    now: i64,
) -> Result<()> {
    require!(!global.paused, ShillbotError::ProtocolPaused);
    require!(
        content_id_len <= MAX_CONTENT_ID_LENGTH,
        ShillbotError::ContentIdTooLong
    );
    require!(
        task.state == TaskState::Claimed,
        ShillbotError::InvalidTaskState
    );
    require!(*expected_agent == task.agent, ShillbotError::NotTaskAgent);
    let submission_deadline = task
        .deadline
        .checked_sub(task.submit_margin)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        now < submission_deadline,
        ShillbotError::SubmitMarginInsufficient
    );
    require!(
        agent_state.agent == *expected_agent,
        ShillbotError::NotTaskAgent
    );
    require!(agent_state.claimed_count > 0, ShillbotError::NoActiveClaim);
    Ok(())
}

/// Common effect phase — decrement claimed_count and write task fields.
pub(crate) fn commit_submit_state(
    agent_state: &mut AgentState,
    task: &mut Task,
    content_id_hash: [u8; 32],
    now: i64,
) -> Result<()> {
    agent_state.claimed_count = agent_state
        .claimed_count
        .checked_sub(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    task.content_id_hash = content_id_hash;
    task.submitted_at = now;
    task.state = TaskState::Submitted;
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
