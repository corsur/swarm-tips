use anchor_lang::prelude::*;

use crate::constants::UNBOND_COOLDOWN_SECS;
use crate::errors::ExtensionRegistryError;
use crate::events::UnbondRequested;
use crate::state::Extension;

/// Pure helper computing `(requested_at, withdrawable_at)` for `request_unbond`
/// while rejecting duplicate unbond requests.
pub fn compute_unbond_timestamps(current_unbond_requested_at: i64, now: i64) -> Result<(i64, i64)> {
    require!(
        current_unbond_requested_at == 0,
        ExtensionRegistryError::UnbondAlreadyRequested
    );
    let requested_at = now.max(1);
    let withdrawable_at = requested_at
        .checked_add(UNBOND_COOLDOWN_SECS)
        .ok_or(ExtensionRegistryError::ArithmeticOverflow)?;
    Ok((requested_at, withdrawable_at))
}

/// Extender initiates the 7-day unbonding cooldown (`UNBOND_COOLDOWN_SECS`).
///
/// While unbonding, the bond remains locked on the `Extension` PDA (and still
/// slashable via `default_extension` if the recipient defaults before the
/// cooldown expires), and `extension-credit` rejects new `open_advance` calls
/// backed by this extension.
pub fn request_unbond(ctx: Context<RequestUnbond>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let extension = &mut ctx.accounts.extension;

    // Checks
    require!(
        extension.extender == ctx.accounts.extender.key(),
        ExtensionRegistryError::InvalidExtender
    );
    require!(
        extension.recipient == ctx.accounts.recipient.key(),
        ExtensionRegistryError::InvalidRecipient
    );

    let (requested_at, withdrawable_at) =
        compute_unbond_timestamps(extension.unbond_requested_at, now)?;

    // Effects
    extension.unbond_requested_at = requested_at;

    emit!(UnbondRequested {
        extender: extension.extender,
        recipient: extension.recipient,
        requested_at,
        withdrawable_at,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct RequestUnbond<'info> {
    #[account(
        mut,
        seeds = [b"extension", extender.key().as_ref(), recipient.key().as_ref()],
        bump = extension.bump,
    )]
    pub extension: Account<'info, Extension>,
    pub extender: Signer<'info>,
    /// CHECK: used only to derive the PDA seed; matched against
    /// `extension.recipient` in the handler.
    pub recipient: UncheckedAccount<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_unbond_timestamps_sets_7_day_cooldown_and_rejects_duplicate() {
        let (req, withdrawable) = compute_unbond_timestamps(0, 1_700_000_000).unwrap();
        assert_eq!(req, 1_700_000_000);
        assert_eq!(withdrawable, 1_700_000_000 + UNBOND_COOLDOWN_SECS);

        // Duplicate request is rejected.
        assert!(compute_unbond_timestamps(req, 1_700_000_100).is_err());
    }
}
