use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::AuthorityTransferred;
use crate::state::GlobalState;

/// Transfers the protocol authority to a new account.
/// Only the current authority can call this instruction.
pub fn transfer_authority(ctx: Context<TransferAuthority>, new_authority: Pubkey) -> Result<()> {
    let global = &ctx.accounts.global_state;

    // Checks: caller is current authority
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Checks: new authority is not the zero key
    require!(
        new_authority != Pubkey::default(),
        ShillbotError::ArithmeticOverflow
    );

    let old_authority = global.authority;

    // Effects
    let global = &mut ctx.accounts.global_state;
    global.authority = new_authority;

    // Interactions
    emit!(AuthorityTransferred {
        old_authority,
        new_authority,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct TransferAuthority<'info> {
    #[account(
        mut,
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
}
