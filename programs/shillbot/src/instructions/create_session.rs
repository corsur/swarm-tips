use anchor_lang::prelude::*;

use crate::events::SessionCreated;
use crate::state::SessionDelegate;

/// Agent creates a session delegation for an MCP server session key.
/// Only the agent (not the delegate) can create the delegation.
pub fn create_session(
    ctx: Context<CreateSession>,
    allowed_instructions: u8,
    duration_seconds: i64,
) -> Result<()> {
    let clock = Clock::get()?;

    // Checks: allowed_instructions is a valid bitmask — must be nonzero
    require!(
        allowed_instructions > 0,
        crate::errors::ShillbotError::InvalidSessionDelegate
    );

    // Checks: duration must be positive
    require!(
        duration_seconds > 0,
        crate::errors::ShillbotError::ArithmeticOverflow
    );

    // Checks: delegate must not be the agent itself (no self-delegation)
    require!(
        ctx.accounts.agent.key() != ctx.accounts.delegate.key(),
        crate::errors::ShillbotError::InvalidSessionDelegate
    );

    let expires_at = clock
        .unix_timestamp
        .checked_add(duration_seconds)
        .ok_or(crate::errors::ShillbotError::ArithmeticOverflow)?;

    // Effects
    let session = &mut ctx.accounts.session_delegate;
    session.agent = ctx.accounts.agent.key();
    session.delegate = ctx.accounts.delegate.key();
    session.allowed_instructions = allowed_instructions;
    session.created_at = clock.unix_timestamp;
    session.expires_at = expires_at;
    session._reserved = [0u8; 32];
    session.bump = ctx.bumps.session_delegate;

    // Postcondition: session fields are correctly set
    require!(
        session.agent == ctx.accounts.agent.key(),
        crate::errors::ShillbotError::InvalidSessionDelegate
    );
    require!(
        session.delegate == ctx.accounts.delegate.key(),
        crate::errors::ShillbotError::InvalidSessionDelegate
    );

    // Interactions: none
    emit!(SessionCreated {
        agent: ctx.accounts.agent.key(),
        delegate: ctx.accounts.delegate.key(),
        allowed_instructions,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct CreateSession<'info> {
    #[account(
        init,
        payer = agent,
        space = SessionDelegate::SPACE,
        seeds = [
            b"session",
            agent.key().as_ref(),
            delegate.key().as_ref(),
        ],
        bump,
    )]
    pub session_delegate: Account<'info, SessionDelegate>,
    #[account(mut)]
    pub agent: Signer<'info>,
    /// CHECK: The delegate pubkey — does not need to sign; the agent authorizes it.
    pub delegate: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}
