use anchor_lang::prelude::*;

use crate::errors::ExtensionRegistryError;
use crate::events::ReturnSubstanceAttested;
use crate::state::Extension;

/// Extender attests the recipient fulfilled the obligation. The `Extension`
/// account is closed via `close = extender`, which returns the rent-exempt
/// minimum AND the bond to the extender in one move — no manual transfer
/// needed (the bond lives on this account).
pub fn attest_return_substance(ctx: Context<AttestReturnSubstance>) -> Result<()> {
    let extension = &ctx.accounts.extension;

    // Checks (also enforced structurally by the seeds + close target).
    require!(
        extension.extender == ctx.accounts.extender.key(),
        ExtensionRegistryError::InvalidExtender
    );
    require!(
        extension.recipient == ctx.accounts.recipient.key(),
        ExtensionRegistryError::InvalidRecipient
    );

    // Effects/Interactions: account close (rent + bond → extender) happens at
    // exit via the `close` constraint. Emit the positive terminal signal.
    emit!(ReturnSubstanceAttested {
        extender: extension.extender,
        recipient: extension.recipient,
        bond_lamports: extension.bond_lamports,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct AttestReturnSubstance<'info> {
    #[account(
        mut,
        close = extender,
        seeds = [b"extension", extender.key().as_ref(), recipient.key().as_ref()],
        bump = extension.bump,
    )]
    pub extension: Account<'info, Extension>,
    #[account(mut)]
    pub extender: Signer<'info>,
    /// CHECK: used only to derive the PDA seed; matched against
    /// `extension.recipient` in the handler.
    pub recipient: UncheckedAccount<'info>,
}
