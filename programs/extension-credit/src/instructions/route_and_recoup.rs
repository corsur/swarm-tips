use anchor_lang::prelude::*;

use crate::errors::ExtensionCreditError;
use crate::events::AdvanceRecouped;
use crate::state::Advance;
use crate::transfers::transfer_lamports;

/// Permissionless crank: distribute the earnings sitting on the Advance PDA
/// (everything above the rent-exempt floor). Backer is recouped first up to the
/// outstanding advance; the recipient gets the remainder. CEI: read → record
/// recoupment (effect) → transfer (interactions).
pub fn route_and_recoup(ctx: Context<RouteAndRecoup>) -> Result<()> {
    let advance_ai = ctx.accounts.advance.to_account_info();
    let rent_floor = Rent::get()?.minimum_balance(advance_ai.data_len());
    let balance = advance_ai.lamports();
    let available = balance.saturating_sub(rent_floor);

    let outstanding = ctx.accounts.advance.outstanding()?;
    let to_backer = available.min(outstanding);
    let to_recipient = available
        .checked_sub(to_backer)
        .ok_or(ExtensionCreditError::ArithmeticOverflow)?;

    let backer = ctx.accounts.advance.backer;
    let recipient = ctx.accounts.advance.recipient;
    let advance_lamports = ctx.accounts.advance.advance_lamports;

    // Effects: record recoupment before moving lamports.
    let total_recouped = {
        let advance = &mut ctx.accounts.advance;
        advance.recouped_lamports = advance
            .recouped_lamports
            .checked_add(to_backer)
            .ok_or(ExtensionCreditError::ArithmeticOverflow)?;
        advance.recouped_lamports
    };

    // Interactions
    if to_backer > 0 {
        transfer_lamports(
            &advance_ai,
            &ctx.accounts.backer.to_account_info(),
            to_backer,
        )?;
    }
    if to_recipient > 0 {
        transfer_lamports(
            &advance_ai,
            &ctx.accounts.recipient.to_account_info(),
            to_recipient,
        )?;
    }

    emit!(AdvanceRecouped {
        backer,
        recipient,
        to_backer,
        to_recipient,
        total_recouped,
        advance_lamports,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct RouteAndRecoup<'info> {
    #[account(
        mut,
        seeds = [b"advance", backer.key().as_ref(), recipient.key().as_ref()],
        bump = advance.bump,
    )]
    pub advance: Account<'info, Advance>,
    /// CHECK: recouped first; bound to advance.backer by the PDA seeds.
    #[account(mut)]
    pub backer: UncheckedAccount<'info>,
    /// CHECK: receives the remainder; bound to advance.recipient by the seeds.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
}
