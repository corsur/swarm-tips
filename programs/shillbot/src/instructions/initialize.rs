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
    switchboard_feed: Pubkey,
) -> Result<()> {
    // Checks
    validate_initialize_inputs(protocol_fee_bps, quality_threshold, switchboard_feed)?;

    // Effects
    init_global_state(
        &mut ctx.accounts.global_state,
        ctx.accounts.authority.key(),
        ctx.accounts.treasury.key(),
        protocol_fee_bps,
        quality_threshold,
        starting_counter,
        switchboard_feed,
        ctx.bumps.global_state,
    );

    // Postconditions: bump is correctly stored (so subsequent instructions
    // re-derive the same PDA) and the authority slot reflects the caller.
    let global = &ctx.accounts.global_state;
    require!(
        global.bump == ctx.bumps.global_state,
        ShillbotError::InvalidTaskState
    );
    require!(
        global.authority == ctx.accounts.authority.key(),
        ShillbotError::NotAuthority
    );

    Ok(())
}

/// Pure check phase. Bounds + non-default-feed.
///
/// The Switchboard feed must be non-default at initialize so `verify_task`
/// has a valid feed from the first task. Operators pre-deploy a feed for
/// the target network and pass its pubkey here; `Pubkey::default()` is
/// rejected as a "deployment is incomplete" tripwire.
fn validate_initialize_inputs(
    protocol_fee_bps: u16,
    quality_threshold: u64,
    switchboard_feed: Pubkey,
) -> Result<()> {
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
    require!(
        switchboard_feed != Pubkey::default(),
        ShillbotError::SwitchboardFeedNotConfigured
    );
    Ok(())
}

/// Pure effect phase. Writes every field of the freshly-initialized
/// GlobalState PDA. `oracle_authority` defaults to the deploy authority;
/// rotate via `update_oracle_authority` after deploy. The `min_escrow_lamports`
/// slot is preserved at zero for binary compat with the 227-byte layout
/// even though no instruction reads it post-2026-05-07.
#[allow(clippy::too_many_arguments)]
fn init_global_state(
    global: &mut GlobalState,
    authority: Pubkey,
    treasury: Pubkey,
    protocol_fee_bps: u16,
    quality_threshold: u64,
    starting_counter: u64,
    switchboard_feed: Pubkey,
    bump: u8,
) {
    global.task_counter = starting_counter;
    global.authority = authority;
    global.treasury = treasury;
    global.protocol_fee_bps = protocol_fee_bps;
    global.quality_threshold = quality_threshold;
    global.challenge_window_seconds = DEFAULT_CHALLENGE_WINDOW_SECONDS;
    global.verification_timeout_seconds = DEFAULT_VERIFICATION_TIMEOUT_SECONDS;
    global.attestation_delay_seconds = DEFAULT_ATTESTATION_DELAY_SECONDS;
    global.staleness_window_seconds = DEFAULT_STALENESS_WINDOW_SECONDS;
    global.max_concurrent_claims = DEFAULT_MAX_CONCURRENT_CLAIMS;
    global.challenge_bond_multiplier_bps = DEFAULT_CHALLENGE_BOND_MULTIPLIER as u16;
    global.bond_slash_treasury_bps = DEFAULT_BOND_SLASH_TREASURY_BPS;
    global.oracle_authority = authority;
    global.paused = false;
    global.paused_platforms = 0;
    global.switchboard_feed = switchboard_feed;
    global.min_escrow_lamports = 0;
    global.rate_limit_window_seconds = crate::constants::RATE_LIMIT_WINDOW_SECONDS;
    global.max_tasks_per_rate_window = crate::constants::MAX_TASKS_PER_RATE_WINDOW;
    global._reserved = [0u8; 12];
    global.bump = bump;
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
