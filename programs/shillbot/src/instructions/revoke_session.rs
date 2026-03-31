use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::SessionRevoked;
use crate::state::SessionDelegate;

/// Agent revokes an MCP session delegation. Only the delegating agent can call.
pub fn revoke_session(ctx: Context<RevokeSession>) -> Result<()> {
    let session = &ctx.accounts.session_delegate;

    // Checks: session belongs to the signing agent (defense-in-depth alongside has_one)
    require!(
        session.agent == ctx.accounts.agent.key(),
        ShillbotError::InvalidSessionDelegate
    );

    // Checks: allowed_instructions is valid (nonzero, within 0x03) — sanity check
    // that the session was properly initialized
    require!(
        session.allowed_instructions > 0 && session.allowed_instructions <= 0x03,
        ShillbotError::InvalidSessionDelegate
    );

    let agent_key = session.agent;
    let delegate_key = session.delegate;

    // Effects: account is closed by the `close` constraint

    // Interactions: none
    emit!(SessionRevoked {
        agent: agent_key,
        delegate: delegate_key,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct RevokeSession<'info> {
    #[account(
        mut,
        close = agent,
        seeds = [
            b"session",
            session_delegate.agent.as_ref(),
            session_delegate.delegate.as_ref(),
        ],
        bump = session_delegate.bump,
        has_one = agent,
    )]
    pub session_delegate: Account<'info, SessionDelegate>,
    #[account(mut)]
    pub agent: Signer<'info>,
}
