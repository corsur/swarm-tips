use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::IdentityRevoked;
use crate::state::PlatformIdentity;

/// Agent revokes their platform identity.
///
/// Closes the PlatformIdentity PDA and returns rent to the agent.
/// Agent must revoke before re-registering with a different platform user ID.
pub fn revoke_identity(ctx: Context<RevokeIdentity>) -> Result<()> {
    let platform = ctx.accounts.identity.platform;
    let agent_key = ctx.accounts.agent.key();

    // Checks: identity belongs to the signing agent. Anchor's `has_one =
    // agent` constraint already enforces this — repeat it here as an
    // explicit invariant so the handler is auditable in isolation.
    require!(
        ctx.accounts.identity.agent == agent_key,
        ShillbotError::NotTaskAgent
    );

    // Checks: platform sanity. The platform byte stored in the PDA must
    // correspond to a known PlatformType. This catches PDAs created
    // against future program versions that introduced a new platform
    // value below ours, then somehow surviving a downgrade.
    require!(
        shared::PlatformType::from_u8(platform).is_some(),
        ShillbotError::InvalidPlatform
    );

    // Interactions (account closure handled by Anchor `close` constraint)
    emit!(IdentityRevoked {
        agent: agent_key,
        platform,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct RevokeIdentity<'info> {
    #[account(
        mut,
        close = agent,
        seeds = [
            b"identity",
            agent.key().as_ref(),
            &[identity.platform],
        ],
        bump = identity.bump,
        has_one = agent @ ShillbotError::NotTaskAgent,
    )]
    pub identity: Account<'info, PlatformIdentity>,
    #[account(mut)]
    pub agent: Signer<'info>,
}
