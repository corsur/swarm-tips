use anchor_lang::prelude::*;

use crate::constants::UNBOND_COOLDOWN_SECS;
use crate::errors::ExtensionRegistryError;
use crate::events::ExtensionWithdrawn;
use crate::state::Extension;

/// Pure cooldown check shared by `withdraw_extension` and unit tests.
pub fn validate_unbond_ready(unbond_requested_at: i64, now: i64) -> Result<()> {
    require!(
        unbond_requested_at > 0,
        ExtensionRegistryError::UnbondNotRequested
    );
    let withdrawable_at = unbond_requested_at
        .checked_add(UNBOND_COOLDOWN_SECS)
        .ok_or(ExtensionRegistryError::ArithmeticOverflow)?;
    require!(
        now >= withdrawable_at,
        ExtensionRegistryError::UnbondCooldownActive
    );
    Ok(())
}

/// The extender withdraws its vouch after completing the 7-day unbond cooldown
/// (`request_unbond` → `UNBOND_COOLDOWN_SECS`). Withdrawing closes the edge
/// and returns the bond + the account's rent to the extender via `close = extender`.
///
/// Requiring the two-step cooldown prevents flash-bond sybil standing attacks
/// and prevents an extender from front-running `default_extension` to rescue
/// a slashable bond (HIGH-16).
pub fn withdraw_extension(ctx: Context<WithdrawExtension>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
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
    validate_unbond_ready(extension.unbond_requested_at, now)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_unbond_ready_enforces_request_and_7_day_cooldown() {
        let t0 = 1_700_000_000i64;
        // Never requested -> rejected.
        assert!(validate_unbond_ready(0, t0 + UNBOND_COOLDOWN_SECS + 100).is_err());

        // 1 second before cooldown ends -> rejected.
        assert!(validate_unbond_ready(t0, t0 + UNBOND_COOLDOWN_SECS - 1).is_err());

        // Exact boundary and after -> accepted.
        assert!(validate_unbond_ready(t0, t0 + UNBOND_COOLDOWN_SECS).is_ok());
        assert!(validate_unbond_ready(t0, t0 + UNBOND_COOLDOWN_SECS + 3_600).is_ok());
    }
}
