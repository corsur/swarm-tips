use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::IdentityRegistered;
use crate::state::PlatformIdentity;

/// Agent registers their platform identity on-chain.
///
/// Creates a PDA binding the agent's wallet to a platform user ID hash.
/// Off-chain challenge verification (e.g., challenge-tweet for X) proves
/// the agent actually controls the platform account.
pub fn register_identity(
    ctx: Context<RegisterIdentity>,
    platform: u8,
    identity_hash: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;

    // Checks: platform is valid
    require!(
        shared::PlatformType::from_u8(platform).is_some(),
        ShillbotError::InvalidPlatform
    );

    // Checks: identity hash is not zero
    require!(identity_hash != [0u8; 32], ShillbotError::InvalidIdentity);

    // Effects
    let identity = &mut ctx.accounts.identity;
    identity.agent = ctx.accounts.agent.key();
    identity.platform = platform;
    identity.identity_hash = identity_hash;
    identity.registered_at = clock.unix_timestamp;
    identity.bump = ctx.bumps.identity;

    // Interactions
    emit!(IdentityRegistered {
        agent: ctx.accounts.agent.key(),
        platform,
        identity_hash,
    });

    Ok(())
}

#[derive(Accounts)]
#[instruction(platform: u8)]
pub struct RegisterIdentity<'info> {
    #[account(
        init,
        payer = agent,
        space = PlatformIdentity::SPACE,
        seeds = [b"identity", agent.key().as_ref(), &[platform]],
        bump,
    )]
    pub identity: Account<'info, PlatformIdentity>,
    #[account(mut)]
    pub agent: Signer<'info>,
    pub system_program: Program<'info, System>,
}
