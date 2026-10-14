use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::constants::MIN_ADVANCE_LAMPORTS;
use crate::errors::ExtensionCreditError;
use crate::events::AdvanceOpened;
use crate::state::Advance;

/// Backer fronts working capital to the recipient and opens the advance (which
/// is also the earnings-routing vault). CEI: validate → record → transfer.
pub fn open_advance(ctx: Context<OpenAdvance>, advance_lamports: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let backer_key = ctx.accounts.backer.key();
    let recipient_key = ctx.accounts.recipient.key();

    // Checks
    require!(
        advance_lamports >= MIN_ADVANCE_LAMPORTS,
        ExtensionCreditError::AdvanceTooLow
    );
    require!(
        backer_key != recipient_key,
        ExtensionCreditError::SelfAdvance
    );

    // Effects
    let advance = &mut ctx.accounts.advance;
    advance.backer = backer_key;
    advance.recipient = recipient_key;
    advance.advance_lamports = advance_lamports;
    advance.recouped_lamports = 0;
    advance.created_at = now;
    advance.bump = ctx.bumps.advance;
    advance._reserved = [0u8; 16];

    // Interactions: if any late-arriving payout landed on the PDA while it was
    // closed prior to re-opening, forward those pre-existing lamports above rent
    // to the recipient so they are never misattributed to the new advance.
    let advance_ai = ctx.accounts.advance.to_account_info();
    let rent_floor = Rent::get()?.minimum_balance(advance_ai.data_len());
    let pre_existing = advance_ai.lamports().saturating_sub(rent_floor);
    if pre_existing > 0 {
        crate::transfers::transfer_lamports(
            &advance_ai,
            &ctx.accounts.recipient.to_account_info(),
            pre_existing,
        )?;
    }

    // Front the capital to the recipient wallet.
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.backer.to_account_info(),
                to: ctx.accounts.recipient.to_account_info(),
            },
        ),
        advance_lamports,
    )?;

    emit!(AdvanceOpened {
        backer: backer_key,
        recipient: recipient_key,
        advance_lamports,
        created_at: now,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct OpenAdvance<'info> {
    #[account(
        init,
        payer = backer,
        space = Advance::SPACE,
        seeds = [b"advance", backer.key().as_ref(), recipient.key().as_ref()],
        bump,
    )]
    pub advance: Account<'info, Advance>,
    #[account(mut)]
    pub backer: Signer<'info>,
    /// CHECK: receives the fronted capital and is used to derive the PDA seed.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
