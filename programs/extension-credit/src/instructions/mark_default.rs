use anchor_lang::prelude::*;

use crate::errors::ExtensionCreditError;
use crate::events::AdvanceDefaulted;
use crate::state::Advance;
use crate::transfers::transfer_lamports;

/// Backer marks a default: sweep any routed earnings to itself as partial
/// recoupment, then close (the backer eats the unrecouped remainder; the rent
/// returns to the backer). CEI: read → record (effect) → sweep (interaction) →
/// close at exit.
pub fn mark_default(ctx: Context<MarkDefault>) -> Result<()> {
    let advance_ai = ctx.accounts.advance.to_account_info();
    let rent_floor = Rent::get()?.minimum_balance(advance_ai.data_len());
    let balance = advance_ai.lamports();
    let available = balance.saturating_sub(rent_floor);

    let backer = ctx.accounts.advance.backer;
    let recipient = ctx.accounts.advance.recipient;
    let advance_lamports = ctx.accounts.advance.advance_lamports;

    // Checks: the close target / seeds already bind the backer; assert the
    // recipient seed matches and the account is in a sane state.
    require!(
        ctx.accounts.recipient.key() == recipient,
        ExtensionCreditError::InvalidRecipient
    );

    let outstanding = ctx.accounts.advance.outstanding()?;
    require!(
        outstanding > 0,
        ExtensionCreditError::AlreadyFullyRecouped
    );

    // Effects: record recoupment, capped at the principal outstanding.
    // Backer sweeps up to outstanding debt; any excess belongs to the recipient.
    let to_backer = available.min(outstanding);
    let to_recipient = available
        .checked_sub(to_backer)
        .ok_or(ExtensionCreditError::ArithmeticOverflow)?;

    let total_recouped = {
        let advance = &mut ctx.accounts.advance;
        advance.recouped_lamports = advance
            .recouped_lamports
            .saturating_add(to_backer)
            .min(advance.advance_lamports);
        advance.recouped_lamports
    };

    // Interactions: sweep backer's recoupment to backer, remainder to recipient;
    // rent returns to backer via `close`.
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

    emit!(AdvanceDefaulted {
        backer,
        recipient,
        swept_to_backer: to_backer,
        total_recouped,
        advance_lamports,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct MarkDefault<'info> {
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
