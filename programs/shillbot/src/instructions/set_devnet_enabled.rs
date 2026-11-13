use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::DevnetEnabledUpdated;
use crate::state::GlobalState;

/// Enables or disables devnet-only verification paths (`verify_task` and kind-0
/// `OnChainAction`). Restricted to `global_state.authority`.
pub fn set_devnet_enabled(ctx: Context<SetDevnetEnabled>, enabled: bool) -> Result<()> {
    require!(
        ctx.accounts.authority.key() == ctx.accounts.global_state.authority,
        ShillbotError::NotAuthority
    );

    ctx.accounts.global_state.devnet_enabled = enabled;

    emit!(DevnetEnabledUpdated {
        devnet_enabled: enabled,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SetDevnetEnabled<'info> {
    #[account(
        mut,
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
}
