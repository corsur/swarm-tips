use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskClaimed;
use crate::state::{AgentState, GlobalState, SessionDelegate, Task, TaskState};

/// Session-delegated variant of claim_task. The delegate (MCP session key)
/// signs instead of the agent. The SessionDelegate PDA must have the
/// claim_task permission bit (0x01) set.
pub fn claim_task_session(ctx: Context<ClaimTaskSession>) -> Result<()> {
    let clock = Clock::get()?;
    let agent_key = ctx.accounts.session_delegate.agent;
    // Checks
    validate_claim_session_inputs(
        &ctx.accounts.global_state,
        &ctx.accounts.session_delegate,
        &ctx.accounts.task,
        &ctx.accounts.agent_state,
        clock.unix_timestamp,
    )?;
    // Effects
    commit_claim_state(
        &mut ctx.accounts.agent_state,
        &mut ctx.accounts.task,
        agent_key,
        ctx.bumps.agent_state,
    )?;
    // Interactions: none
    emit!(TaskClaimed {
        task_id: ctx.accounts.task.task_id,
        agent: agent_key,
    });
    Ok(())
}

/// Pure check phase. Protocol-paused, session-bitmask-has-claim,
/// session-not-expired, task-Open, claim-buffer satisfied,
/// concurrent-claim-limit not exceeded.
fn validate_claim_session_inputs(
    global: &GlobalState,
    session: &SessionDelegate,
    task: &Task,
    agent_state: &AgentState,
    now: i64,
) -> Result<()> {
    require!(!global.paused, ShillbotError::ProtocolPaused);
    require!(
        session.allowed_instructions & 0x01 != 0,
        ShillbotError::InvalidSessionDelegate
    );
    if session.expires_at > 0 {
        require!(now < session.expires_at, ShillbotError::SessionExpired);
    }
    require!(
        task.state == TaskState::Open,
        ShillbotError::InvalidTaskState
    );
    // Deterministic (kind 1) tasks reject self-claims — same arms-length
    // rule as claim_task, keyed on the DELEGATING agent, not the session key.
    if task.verification_kind == 1 {
        require!(
            session.agent != task.client,
            ShillbotError::SelfClaimForbidden
        );
    }
    let earliest_claim_deadline = now
        .checked_add(task.claim_buffer)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        earliest_claim_deadline < task.deadline,
        ShillbotError::ClaimBufferInsufficient
    );
    require!(
        agent_state.claimed_count < global.max_concurrent_claims,
        ShillbotError::MaxConcurrentClaimsExceeded
    );
    Ok(())
}

/// Pure effect phase. Initialize the AgentState slots on first claim,
/// increment claimed_count, transition task → Claimed.
fn commit_claim_state(
    agent_state: &mut AgentState,
    task: &mut Task,
    agent_key: Pubkey,
    agent_state_bump: u8,
) -> Result<()> {
    if agent_state.agent == Pubkey::default() {
        agent_state.agent = agent_key;
        agent_state.bump = agent_state_bump;
    }
    agent_state.claimed_count = agent_state
        .claimed_count
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    task.agent = agent_key;
    task.state = TaskState::Claimed;
    Ok(())
}

#[derive(Accounts)]
pub struct ClaimTaskSession<'info> {
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
    /// Uses `init_if_needed` — see claim_task.rs for justification.
    #[account(
        init_if_needed,
        payer = payer,
        space = AgentState::SPACE,
        seeds = [b"agent_state", session_delegate.agent.as_ref()],
        bump,
    )]
    pub agent_state: Account<'info, AgentState>,
    /// SessionDelegate PDA proves the delegate is authorized by the agent.
    #[account(
        seeds = [
            b"session",
            session_delegate.agent.as_ref(),
            delegate.key().as_ref(),
        ],
        bump = session_delegate.bump,
    )]
    pub session_delegate: Account<'info, SessionDelegate>,
    /// The session key (MCP server) that signs the transaction.
    pub delegate: Signer<'info>,
    /// Pays for AgentState init if needed. Typically the delegate or a relayer.
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
