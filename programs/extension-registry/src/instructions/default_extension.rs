use anchor_lang::prelude::*;

use crate::errors::ExtensionRegistryError;
use crate::events::ExtensionDefaulted;
use crate::state::{Extension, GlobalState};
use crate::transfers::transfer_lamports;

/// Authority (from `GlobalState`) arbitrates a default: the bond is slashed to
/// the configured treasury and the account closes, returning only the rent to
/// the extender.
///
/// CEI: checks (authority/treasury/extender/recipient identity) → no state
/// mutation → interactions (slash bond, then `close = extender` at exit moves
/// the remaining rent).
pub fn default_extension(ctx: Context<DefaultExtension>) -> Result<()> {
    let extension = &ctx.accounts.extension;
    let global = &ctx.accounts.global_state;
    let bond = extension.bond_lamports;

    // Checks
    require!(
        ctx.accounts.authority.key() == global.authority,
        ExtensionRegistryError::NotAuthority
    );
    require!(
        ctx.accounts.treasury.key() == global.treasury,
        ExtensionRegistryError::InvalidTreasury
    );
    require!(
        extension.extender == ctx.accounts.extender.key(),
        ExtensionRegistryError::InvalidExtender
    );
    require!(
        extension.recipient == ctx.accounts.recipient.key(),
        ExtensionRegistryError::InvalidRecipient
    );

    let extender = extension.extender;
    let recipient = extension.recipient;

    // Interactions: slash the bond to the treasury. The remaining lamports
    // (rent) go to the extender when the `close` constraint runs at exit.
    transfer_lamports(
        &ctx.accounts.extension.to_account_info(),
        &ctx.accounts.treasury.to_account_info(),
        bond,
    )?;

    emit!(ExtensionDefaulted {
        extender,
        recipient,
        bond_lamports_slashed: bond,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct DefaultExtension<'info> {
    #[account(
        mut,
        close = extender,
        seeds = [b"extension", extender.key().as_ref(), recipient.key().as_ref()],
        bump = extension.bump,
    )]
    pub extension: Account<'info, Extension>,
    #[account(
        seeds = [b"extension_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
    /// CHECK: receives the slashed bond; validated against `global_state.treasury`.
    #[account(mut)]
    pub treasury: UncheckedAccount<'info>,
    /// CHECK: receives the returned rent via `close`; validated against
    /// `extension.extender` and used to derive the PDA seed.
    #[account(mut)]
    pub extender: UncheckedAccount<'info>,
    /// CHECK: used only to derive the PDA seed; matched against
    /// `extension.recipient`.
    pub recipient: UncheckedAccount<'info>,
}
