use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskClaimed;
use crate::state::{AgentState, GlobalState, SessionDelegate, Task, TaskState};

/// Session-delegated variant of claim_task. The delegate (MCP session key)
/// signs instead of the agent. The SessionDelegate PDA must have the
/// claim_task permission bit (0x01) set.
pub fn claim_task_session(ctx: Context<ClaimTaskSession>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let agent_state = &ctx.accounts.agent_state;
    let session = &ctx.accounts.session_delegate;
    let global = &ctx.accounts.global_state;

    // Checks: protocol not paused
    require!(!global.paused, ShillbotError::ProtocolPaused);

    // Checks: session has claim_task permission (bit 0)
    require!(
        session.allowed_instructions & 0x01 != 0,
        ShillbotError::InvalidSessionDelegate
    );

    // Checks: session not expired (expires_at == 0 means no expiry)
    if session.expires_at > 0 {
        require!(
            clock.unix_timestamp < session.expires_at,
            ShillbotError::SessionExpired
        );
    }

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
    let agent_key = session.agent;
    // Only set agent and bump on first initialization (freshly zeroed account)
    if agent_state.agent == Pubkey::default() {
        agent_state.agent = agent_key;
        agent_state.bump = ctx.bumps.agent_state;
    }
    agent_state.claimed_count = agent_state
        .claimed_count
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
