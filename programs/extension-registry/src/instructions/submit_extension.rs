use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::constants::{EXTENSION_TYPE_CAPABILITY_VALIDATION, MIN_BOND_LAMPORTS};
use crate::errors::ExtensionRegistryError;
use crate::events::ExtensionSubmitted;
use crate::state::Extension;

/// Extender locks a bond and records an obligation-creating extension.
///
/// Thin handler: validate → record state → transfer the bond (CEI). The bond
/// lands on the `Extension` PDA (on top of the rent the extender pays at init),
/// where it stays until the obligation resolves.
pub fn submit_extension(
    ctx: Context<SubmitExtension>,
    extension_type: u8,
    bond_lamports: u64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let extender_key = ctx.accounts.extender.key();
    let recipient_key = ctx.accounts.recipient.key();

    // Checks
    require!(
        extension_type == EXTENSION_TYPE_CAPABILITY_VALIDATION,
        ExtensionRegistryError::UnsupportedExtensionType
    );
    require!(
        bond_lamports >= MIN_BOND_LAMPORTS,
        ExtensionRegistryError::BondTooLow
    );
    require!(
        extender_key != recipient_key,
        ExtensionRegistryError::SelfExtension
    );

    // Effects
    let extension = &mut ctx.accounts.extension;
    extension.extender = extender_key;
    extension.recipient = recipient_key;
    extension.extension_type = extension_type;
    extension.bond_lamports = bond_lamports;
    extension.created_at = now;
    extension.bump = ctx.bumps.extension;
    extension._reserved = [0u8; 16];

    // Interactions: move the bond from the extender wallet into the PDA.
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.extender.to_account_info(),
                to: ctx.accounts.extension.to_account_info(),
            },
        ),
        bond_lamports,
    )?;

    emit!(ExtensionSubmitted {
        extender: extender_key,
        recipient: recipient_key,
        extension_type,
        bond_lamports,
        created_at: now,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct SubmitExtension<'info> {
    #[account(
        init,
        payer = extender,
        space = Extension::SPACE,
        seeds = [b"extension", extender.key().as_ref(), recipient.key().as_ref()],
        bump,
    )]
    pub extension: Account<'info, Extension>,
    #[account(mut)]
    pub extender: Signer<'info>,
    /// CHECK: used only to derive the PDA seed and as the recorded recipient;
    /// no data is read.
    pub recipient: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
