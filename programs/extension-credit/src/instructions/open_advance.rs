use anchor_lang::prelude::*;
use anchor_lang::system_program;
use extension_registry::constants::MIN_BOND_LAMPORTS;
use extension_registry::state::Extension;

use crate::constants::MIN_ADVANCE_LAMPORTS;
use crate::errors::ExtensionCreditError;
use crate::events::AdvanceOpened;
use crate::state::Advance;

/// Pure validation that a live, non-unbonding `Extension` backs `(backer, recipient)`.
pub fn validate_backing_extension(
    extension: &Extension,
    backer: &Pubkey,
    recipient: &Pubkey,
) -> Result<()> {
    require!(
        extension.extender == *backer && extension.recipient == *recipient,
        ExtensionCreditError::MissingActiveExtension
    );
    require!(
        !extension.is_unbonding(),
        ExtensionCreditError::ExtensionUnbonding
    );
    require!(
        extension.bond_lamports >= MIN_BOND_LAMPORTS,
        ExtensionCreditError::InsufficientExtensionBond
    );
    Ok(())
}

/// Backer fronts working capital to the recipient and opens the advance (which
/// is also the earnings-routing vault). Requires an active, non-unbonding
/// `extension-registry` vouch (`Extension` PDA) from `backer` to `recipient`
/// (HIGH-16). CEI: validate → record → transfer.
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
    validate_backing_extension(&ctx.accounts.extension, &backer_key, &recipient_key)?;

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
    #[account(
        seeds = [b"extension", backer.key().as_ref(), recipient.key().as_ref()],
        bump = extension.bump,
        seeds::program = extension_registry::ID,
    )]
    pub extension: Account<'info, Extension>,
    #[account(mut)]
    pub backer: Signer<'info>,
    /// CHECK: receives the fronted capital and is used to derive the PDA seed.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_extension(extender: Pubkey, recipient: Pubkey) -> Extension {
        Extension {
            extender,
            recipient,
            extension_type: 0,
            bond_lamports: MIN_BOND_LAMPORTS,
            created_at: 1_700_000_000,
            bump: 254,
            unbond_requested_at: 0,
            _reserved: [0u8; 8],
        }
    }

    #[test]
    fn validate_backing_extension_accepts_active_and_rejects_unbonding_or_mismatched() {
        let backer = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let mut ext = sample_extension(backer, recipient);

        assert!(validate_backing_extension(&ext, &backer, &recipient).is_ok());

        // Mismatched backer or recipient -> rejected.
        assert!(validate_backing_extension(&ext, &Pubkey::new_unique(), &recipient).is_err());
        assert!(validate_backing_extension(&ext, &backer, &Pubkey::new_unique()).is_err());

        // Unbonding in progress -> rejected.
        ext.unbond_requested_at = 1_700_000_500;
        assert!(validate_backing_extension(&ext, &backer, &recipient).is_err());

        // Below minimum bond -> rejected.
        ext.unbond_requested_at = 0;
        ext.bond_lamports = MIN_BOND_LAMPORTS - 1;
        assert!(validate_backing_extension(&ext, &backer, &recipient).is_err());
    }
}
