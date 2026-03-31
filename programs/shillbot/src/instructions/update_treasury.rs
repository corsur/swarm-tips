use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TreasuryUpdated;
use crate::state::GlobalState;

/// Updates the treasury account. Only the authority can call this instruction.
pub fn update_treasury(ctx: Context<UpdateTreasury>, new_treasury: Pubkey) -> Result<()> {
    let global = &ctx.accounts.global_state;

    // Checks: caller is authority
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Checks: new treasury is not the zero key
    require!(
        new_treasury != Pubkey::default(),
        ShillbotError::ArithmeticOverflow
    );

    let old_treasury = global.treasury;

    // Effects
    let global = &mut ctx.accounts.global_state;
    global.treasury = new_treasury;

    // Interactions
    emit!(TreasuryUpdated {
        old_treasury,
        new_treasury,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct UpdateTreasury<'info> {
    #[account(
        mut,
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
}
