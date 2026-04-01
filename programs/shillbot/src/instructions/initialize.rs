use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::state::GlobalState;
use crate::{
    DEFAULT_ATTESTATION_DELAY_SECONDS, DEFAULT_BOND_SLASH_TREASURY_BPS,
    DEFAULT_CHALLENGE_BOND_MULTIPLIER, DEFAULT_CHALLENGE_WINDOW_SECONDS,
    DEFAULT_MAX_CONCURRENT_CLAIMS, DEFAULT_STALENESS_WINDOW_SECONDS,
    DEFAULT_VERIFICATION_TIMEOUT_SECONDS,
};

/// One-time initialization to create the GlobalState singleton.
pub fn initialize(
    ctx: Context<Initialize>,
    protocol_fee_bps: u16,
    quality_threshold: u64,
    starting_counter: u64,
) -> Result<()> {
    // Checks
    require!(
        protocol_fee_bps >= shared::MIN_PROTOCOL_FEE_BPS,
        ShillbotError::ProtocolFeeBoundsExceeded
    );
    require!(
        protocol_fee_bps <= shared::MAX_PROTOCOL_FEE_BPS,
        ShillbotError::ProtocolFeeBoundsExceeded
    );
    require!(
        quality_threshold <= shared::MAX_SCORE,
        ShillbotError::QualityThresholdBoundsExceeded
    );

    // Effects
    let global = &mut ctx.accounts.global_state;
    global.task_counter = starting_counter;
    global.authority = ctx.accounts.authority.key();
    global.treasury = ctx.accounts.treasury.key();
    global.protocol_fee_bps = protocol_fee_bps;
    global.quality_threshold = quality_threshold;
    global.challenge_window_seconds = DEFAULT_CHALLENGE_WINDOW_SECONDS;
    global.verification_timeout_seconds = DEFAULT_VERIFICATION_TIMEOUT_SECONDS;
    global.attestation_delay_seconds = DEFAULT_ATTESTATION_DELAY_SECONDS;
    global.staleness_window_seconds = DEFAULT_STALENESS_WINDOW_SECONDS;
    global.max_concurrent_claims = DEFAULT_MAX_CONCURRENT_CLAIMS;
    global.challenge_bond_multiplier_bps = DEFAULT_CHALLENGE_BOND_MULTIPLIER as u16;
    global.bond_slash_treasury_bps = DEFAULT_BOND_SLASH_TREASURY_BPS;
    global.oracle_authority = ctx.accounts.authority.key();
    global.paused = false;
    global.paused_platforms = 0;
    global.switchboard_feed = Pubkey::default();
    global._reserved = [0u8; 32];
    global.bump = ctx.bumps.global_state;

    Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = GlobalState::SPACE,
        seeds = [b"shillbot_global"],
        bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: Treasury address stored in GlobalState for fee collection.
    pub treasury: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}
