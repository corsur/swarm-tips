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
    // The Switchboard feed must be set at initialize so verify_task has
    // a valid feed to check against from the first task. Operators
    // pre-deploy a Switchboard pull feed for the target network and
    // pass its pubkey here. Pubkey::default() is rejected — the field
    // serves as a "deployment is incomplete" tripwire (verify_task
    // refuses to run if the feed is zero).
    require!(
        switchboard_feed != Pubkey::default(),
        ShillbotError::SwitchboardFeedNotConfigured
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
    // Switchboard feed is set from the per-network arg above (validated
    // non-default). Operators pre-deploy a feed for each target network
    // and pass its pubkey at initialize. Rotation post-deploy requires
    // a program upgrade until `set_switchboard_feed` is re-added.
    global.switchboard_feed = switchboard_feed;
    // D3 (2026-05-07): rate-limit defaults match the pre-D3 const values
    // so initialize-then-update_params is a clean transition.
    // The D2 `min_escrow_lamports` slot stays zero — the gate was removed
    // 2026-05-07 in favor of the EigenTrust reputation-graph defense.
    global.min_escrow_lamports = 0;
    global.rate_limit_window_seconds = crate::constants::RATE_LIMIT_WINDOW_SECONDS;
    global.max_tasks_per_rate_window = crate::constants::MAX_TASKS_PER_RATE_WINDOW;
    global._reserved = [0u8; 12];
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
