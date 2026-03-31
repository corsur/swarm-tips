use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::OracleAuthorityUpdated;
use crate::state::GlobalState;

/// Updates the oracle authority. Only the protocol authority can call this instruction.
pub fn update_oracle_authority(
    ctx: Context<UpdateOracleAuthority>,
    new_oracle_authority: Pubkey,
) -> Result<()> {
    let global = &ctx.accounts.global_state;

    // Checks: caller is authority
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Checks: new oracle authority is not the zero key
    require!(
        new_oracle_authority != Pubkey::default(),
        ShillbotError::ArithmeticOverflow
    );

    let old_oracle_authority = global.oracle_authority;

    // Effects
    let global = &mut ctx.accounts.global_state;
    global.oracle_authority = new_oracle_authority;

    // Interactions
    emit!(OracleAuthorityUpdated {
        old_oracle_authority,
        new_oracle_authority,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct UpdateOracleAuthority<'info> {
    #[account(
        mut,
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
}
