use anchor_lang::prelude::*;

use crate::errors::ExtensionCreditError;
use crate::events::AdvanceClosed;
use crate::state::Advance;

/// Backer closes a fully-recouped, drained advance, reclaiming the rent.
pub fn close_advance(ctx: Context<CloseAdvance>) -> Result<()> {
    let advance_ai = ctx.accounts.advance.to_account_info();
    let rent_floor = Rent::get()?.minimum_balance(advance_ai.data_len());
    let balance = advance_ai.lamports();

    // Checks: fully recouped, and the vault holds nothing but rent.
    require!(
        ctx.accounts.advance.outstanding()? == 0,
        ExtensionCreditError::NotFullyRecouped
    );
    require!(balance <= rent_floor, ExtensionCreditError::VaultNotDrained);

    emit!(AdvanceClosed {
        backer: ctx.accounts.advance.backer,
        recipient: ctx.accounts.advance.recipient,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct CloseAdvance<'info> {
    #[account(
        mut,
        close = backer,
        seeds = [b"advance", backer.key().as_ref(), recipient.key().as_ref()],
        bump = advance.bump,
    )]
    pub advance: Account<'info, Advance>,
    #[account(mut)]
    pub backer: Signer<'info>,
    /// CHECK: used to derive the PDA seed; bound to advance.recipient.
    pub recipient: UncheckedAccount<'info>,
}
