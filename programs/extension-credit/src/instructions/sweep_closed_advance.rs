use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::errors::ExtensionCreditError;
use crate::events::ClosedAdvanceSwept;

/// Permissionless recovery crank (NEW-HIGH-02): if an in-flight Shillbot task
/// routed `payout_to` to an `Advance` PDA that was closed before the task
/// finalized, any lamports sitting on the closed (System-owned, 0-byte) PDA
/// can be swept directly to `recipient` using PDA signer seeds.
pub fn sweep_closed_advance(ctx: Context<SweepClosedAdvance>) -> Result<()> {
    let vault_ai = ctx.accounts.advance_pda.to_account_info();
    require!(
        vault_ai.owner == &system_program::ID && vault_ai.data_is_empty(),
        ExtensionCreditError::InvalidLamportSource
    );
    let amount = vault_ai.lamports();
    if amount == 0 {
        return Ok(());
    }

    let backer_key = ctx.accounts.backer.key();
    let recipient_key = ctx.accounts.recipient.key();
    let bump = ctx.bumps.advance_pda;
    let signer_seeds: &[&[&[u8]]] = &[&[
        b"advance",
        backer_key.as_ref(),
        recipient_key.as_ref(),
        &[bump],
    ]];

    system_program::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: vault_ai,
                to: ctx.accounts.recipient.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )?;

    emit!(ClosedAdvanceSwept {
        backer: backer_key,
        recipient: recipient_key,
        amount,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SweepClosedAdvance<'info> {
    /// CHECK: Uninitialized / closed Advance PDA (`owner == SystemProgram`,
    /// `data_is_empty()`), verified by PDA seeds and handler check.
    #[account(
        mut,
        seeds = [b"advance", backer.key().as_ref(), recipient.key().as_ref()],
        bump,
    )]
    pub advance_pda: UncheckedAccount<'info>,
    /// CHECK: PDA seed binding only.
    pub backer: UncheckedAccount<'info>,
    /// CHECK: PDA seed binding and recipient of swept earnings.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
