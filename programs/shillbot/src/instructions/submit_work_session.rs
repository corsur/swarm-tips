use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::WorkSubmitted;
use crate::instructions::submit_work::{commit_submit_state, validate_submit_common};
use crate::state::{AgentState, GlobalState, SessionDelegate, Task};

/// Session-delegated variant of submit_work. The delegate (MCP session key)
/// signs instead of the agent. The SessionDelegate PDA must have the
/// submit_work permission bit (0x02) set.
pub fn submit_work_session(ctx: Context<SubmitWorkSession>, content_id: Vec<u8>) -> Result<()> {
    let clock = Clock::get()?;
    let session = &ctx.accounts.session_delegate;

    // Checks: session-specific (bitmask + expiry).
    validate_submit_session_constraints(session, clock.unix_timestamp)?;

    // Checks: shared with submit_work — protocol-paused, content-id bound,
    // state, agent identity (using session.agent), deadline, claimed_count.
    validate_submit_common(
        &ctx.accounts.global_state,
        &ctx.accounts.task,
        &session.agent,
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
        agent: ctx.accounts.session_delegate.agent,
        content_id_hash,
    });

    Ok(())
}

/// Session-specific check phase: bitmask grants submit_work (bit 1), and
/// the session is not expired (`expires_at == 0` means "no expiry").
fn validate_submit_session_constraints(session: &SessionDelegate, now: i64) -> Result<()> {
    require!(
        session.allowed_instructions & 0x02 != 0,
        ShillbotError::InvalidSessionDelegate
    );
    if session.expires_at > 0 {
        require!(now < session.expires_at, ShillbotError::SessionExpired);
    }
    Ok(())
}

#[derive(Accounts)]
pub struct SubmitWorkSession<'info> {
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
        seeds = [b"agent_state", session_delegate.agent.as_ref()],
        bump = agent_state.bump,
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
}
