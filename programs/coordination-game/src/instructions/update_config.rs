use anchor_lang::prelude::*;

use crate::errors::CoordinationError;
use crate::events::ConfigUpdated;
use crate::state::global_config::{GlobalConfig, MAX_TREASURY_SPLIT_BPS, MIN_TREASURY_SPLIT_BPS};

/// Authority-gated: update GlobalConfig parameters.
///
/// All Pubkey fields use the "zero-key means skip" convention: if a Pubkey
/// parameter equals `Pubkey::default()` (all zeros), that field is not updated.
/// This avoids Option in instruction data.
pub fn update_config(
    ctx: Context<UpdateConfig>,
    treasury_split_bps: u16,
    treasury: Pubkey,
    matchmaker: Pubkey,
    new_authority: Pubkey,
) -> Result<()> {
    let config = &ctx.accounts.global_config;

    // Checks
    require!(
        ctx.accounts.authority.key() == config.authority,
        CoordinationError::NotAuthority
    );
    require!(
        (MIN_TREASURY_SPLIT_BPS..=MAX_TREASURY_SPLIT_BPS).contains(&treasury_split_bps),
        CoordinationError::InvalidTreasurySplitBps
    );
    // If new_authority is provided (non-default), it must not be the zero key.
    // This is a tautology by construction (non-default IS non-zero), but we
    // assert explicitly as defense-in-depth against future refactors.
    if new_authority != Pubkey::default() {
        require!(
            new_authority != Pubkey::default(),
            CoordinationError::InvalidGameState
        );
    }

    // Effects
    let config = &mut ctx.accounts.global_config;
    config.treasury_split_bps = treasury_split_bps;

    if treasury != Pubkey::default() {
        config.treasury = treasury;
    }
    if matchmaker != Pubkey::default() {
        config.matchmaker = matchmaker;
    }
    if new_authority != Pubkey::default() {
        config.authority = new_authority;
    }

    // Postconditions
    require!(
        config.treasury_split_bps >= MIN_TREASURY_SPLIT_BPS
            && config.treasury_split_bps <= MAX_TREASURY_SPLIT_BPS,
        CoordinationError::InvalidTreasurySplitBps
    );
    require!(
        config.authority != Pubkey::default(),
        CoordinationError::InvalidGameState
    );

    emit!(ConfigUpdated {
        authority: config.authority,
        treasury_split_bps: config.treasury_split_bps,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    #[account(
        mut,
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    pub authority: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the zero-key sentinel is distinct from any valid Pubkey.
    #[test]
    fn default_pubkey_is_all_zeros() {
        let zero = Pubkey::default();
        assert_eq!(zero, Pubkey::new_from_array([0u8; 32]));
    }

    /// Verify treasury_split_bps bounds are sane (compile-time check).
    #[test]
    fn treasury_split_bounds_are_ordered() {
        // Use const assertions via array indexing (compile-time panics on violation)
        const _: () = assert!(MIN_TREASURY_SPLIT_BPS < MAX_TREASURY_SPLIT_BPS);
        const _: () = assert!(MIN_TREASURY_SPLIT_BPS > 0);
        const _: () = assert!(MAX_TREASURY_SPLIT_BPS <= 10_000);
    }
}
