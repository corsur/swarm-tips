use anchor_lang::prelude::*;

use crate::state::DEFAULT_STAKE_LAMPORTS;

use crate::errors::CoordinationError;
use crate::state::global_config::{GlobalConfig, MAX_TREASURY_SPLIT_BPS, MIN_TREASURY_SPLIT_BPS};

/// One-time setup: creates the GlobalConfig singleton PDA.
pub fn initialize_config(ctx: Context<InitializeConfig>, treasury_split_bps: u16) -> Result<()> {
    // Checks
    require!(
        (MIN_TREASURY_SPLIT_BPS..=MAX_TREASURY_SPLIT_BPS).contains(&treasury_split_bps),
        CoordinationError::InvalidTreasurySplitBps
    );
    // Reject zero pubkeys for matchmaker and treasury — Pubkey::default()
    // is the system program ID and never a legitimate destination for
    // funds (treasury) or a legitimate signer (matchmaker).
    require!(
        ctx.accounts.matchmaker.key() != Pubkey::default(),
        CoordinationError::NotMatchmaker
    );
    require!(
        ctx.accounts.treasury.key() != Pubkey::default(),
        CoordinationError::InvalidTreasury
    );

    // Effects
    let config = &mut ctx.accounts.global_config;
    config.authority = ctx.accounts.authority.key();
    config.matchmaker = ctx.accounts.matchmaker.key();
    config.treasury = ctx.accounts.treasury.key();
    config.treasury_split_bps = treasury_split_bps;
    config.bump = ctx.bumps.global_config;
    // A freshly initialized config must carry a USABLE stake. Leaving this at
    // the u64 default of 0 made every create_game fail StakeMismatch, because
    // the handler now compares against the config rather than a constant —
    // caught by the bankrun matrix, which is exactly what it is for.
    config.stake_lamports = DEFAULT_STAKE_LAMPORTS;

    // Postcondition
    require!(
        config.treasury_split_bps >= MIN_TREASURY_SPLIT_BPS
            && config.treasury_split_bps <= MAX_TREASURY_SPLIT_BPS,
        CoordinationError::InvalidTreasurySplitBps
    );

    Ok(())
}

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(
        init,
        payer = authority,
        space = GlobalConfig::SPACE,
        seeds = [b"global_config"],
        bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: Matchmaker pubkey, stored in config.
    pub matchmaker: AccountInfo<'info>,
    /// CHECK: Treasury pubkey, stored in config.
    pub treasury: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[cfg(test)]
mod tests {
    use crate::state::{DEFAULT_STAKE_LAMPORTS, MAX_STAKE_LAMPORTS, MIN_STAKE_LAMPORTS};

    #[test]
    fn a_fresh_config_seeds_a_usable_stake() {
        // Regression: when stake_lamports was added, initialize_config did not
        // write it, so a fresh config carried the u64 default of 0 and EVERY
        // create_game failed StakeMismatch. Unit tests passed; the bankrun
        // matrix caught it. A zero stake must never be the seeded value.
        assert_ne!(DEFAULT_STAKE_LAMPORTS, 0);
        assert!((MIN_STAKE_LAMPORTS..=MAX_STAKE_LAMPORTS).contains(&DEFAULT_STAKE_LAMPORTS));
    }
}
