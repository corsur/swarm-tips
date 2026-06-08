use anchor_lang::prelude::*;

use crate::errors::ExtensionRegistryError;
use crate::events::ExtensionWithdrawn;
use crate::state::Extension;

/// The extender withdraws its vouch. An extension is a *live, collateralized*
/// edge: while it exists the recipient holds borrowed standing; withdrawing it
/// removes the edge (the recipient's standing drops) and returns the bond + the
/// account's rent to the extender via `close = extender`.
///
/// This replaces the old `attest_return_substance`, which emitted a "recipient
/// fulfilled the obligation" signal the program never actually verified.
/// Withdrawal makes no such unproven claim — it is simply the extender
/// reclaiming a vouch it no longer chooses to back (e.g. once the recipient is
/// self-sustaining, so the protocol recycles the onboarding bond). Slashing a
/// defaulted recipient remains `default_extension`.
pub fn withdraw_extension(ctx: Context<WithdrawExtension>) -> Result<()> {
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

    // Effects/Interactions: the account close (rent + bond → extender) happens
    // at exit via the `close` constraint. Emit the terminal withdrawal signal so
    // the off-chain web-position indexer drops the edge (standing recomputes).
    emit!(ExtensionWithdrawn {
        extender: extension.extender,
        recipient: extension.recipient,
        bond_lamports: extension.bond_lamports,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawExtension<'info> {
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
