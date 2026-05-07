use anchor_lang::prelude::*;

use crate::constants::{
    MAX_RATE_LIMIT_WINDOW_SECONDS, MAX_TASKS_PER_RATE_WINDOW_CEILING,
    MIN_RATE_LIMIT_WINDOW_SECONDS, MIN_TASKS_PER_RATE_WINDOW,
};
use crate::errors::ShillbotError;
use crate::events::ParamsUpdated;
use crate::state::GlobalState;
use crate::{MAX_CHALLENGE_BOND_MULTIPLIER, MIN_CHALLENGE_BOND_MULTIPLIER};

/// Authority-only instruction to update tunable protocol parameters.
#[allow(clippy::too_many_arguments)]
pub fn update_params(
    ctx: Context<UpdateParams>,
    protocol_fee_bps: u16,
    quality_threshold: u64,
    challenge_window_seconds: i64,
    verification_timeout_seconds: i64,
    attestation_delay_seconds: i64,
    staleness_window_seconds: i64,
    max_concurrent_claims: u8,
    challenge_bond_multiplier: u8,
    bond_slash_treasury_bps: u16,
    paused: bool,
    paused_platforms: u16,
    // D3 (2026-05-07): per-client task-creation rate limit, governance-
    // controllable with bounds enforced below. The D2 `min_escrow_lamports`
    // param was removed 2026-05-07; the GlobalState slot is preserved as
    // vestigial-zero for binary compat with deployed 227-byte accounts.
    rate_limit_window_seconds: i64,
    max_tasks_per_rate_window: u32,
) -> Result<()> {
    let global = &ctx.accounts.global_state;

    // Checks: caller is authority
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Checks: fee within bounds [100, 2500] bps
    require!(
        protocol_fee_bps >= shared::MIN_PROTOCOL_FEE_BPS,
        ShillbotError::ProtocolFeeBoundsExceeded
    );
    require!(
        protocol_fee_bps <= shared::MAX_PROTOCOL_FEE_BPS,
        ShillbotError::ProtocolFeeBoundsExceeded
    );

    // Checks: threshold within bounds
    require!(
        quality_threshold <= shared::MAX_SCORE,
        ShillbotError::QualityThresholdBoundsExceeded
    );

    // Checks: timing parameters must be positive
    require!(
        challenge_window_seconds > 0,
        ShillbotError::ArithmeticOverflow
    );
    require!(
        verification_timeout_seconds > 0,
        ShillbotError::ArithmeticOverflow
    );
    require!(
        attestation_delay_seconds > 0,
        ShillbotError::ArithmeticOverflow
    );
    require!(
        staleness_window_seconds > 0,
        ShillbotError::ArithmeticOverflow
    );

    // Checks: max_concurrent_claims in [1, 255]
    require!(max_concurrent_claims > 0, ShillbotError::ArithmeticOverflow);

    // Checks: challenge bond multiplier within bounds [2, 10]
    require!(
        challenge_bond_multiplier >= MIN_CHALLENGE_BOND_MULTIPLIER,
        ShillbotError::InsufficientBond
    );
    require!(
        challenge_bond_multiplier <= MAX_CHALLENGE_BOND_MULTIPLIER,
        ShillbotError::InsufficientBond
    );

    // Checks: bond_slash_treasury_bps within [0, 10000]
    require!(
        bond_slash_treasury_bps <= 10_000,
        ShillbotError::ProtocolFeeBoundsExceeded
    );

    // Checks (D3): rate-limit window within bounds
    require!(
        rate_limit_window_seconds >= MIN_RATE_LIMIT_WINDOW_SECONDS,
        ShillbotError::RateLimitExceeded
    );
    require!(
        rate_limit_window_seconds <= MAX_RATE_LIMIT_WINDOW_SECONDS,
        ShillbotError::RateLimitExceeded
    );

    // Checks (D3): max_tasks_per_rate_window within bounds
    require!(
        max_tasks_per_rate_window >= MIN_TASKS_PER_RATE_WINDOW,
        ShillbotError::RateLimitExceeded
    );
    require!(
        max_tasks_per_rate_window <= MAX_TASKS_PER_RATE_WINDOW_CEILING,
        ShillbotError::RateLimitExceeded
    );

    // Effects
    let global = &mut ctx.accounts.global_state;
    global.protocol_fee_bps = protocol_fee_bps;
    global.quality_threshold = quality_threshold;
    global.challenge_window_seconds = challenge_window_seconds;
    global.verification_timeout_seconds = verification_timeout_seconds;
    global.attestation_delay_seconds = attestation_delay_seconds;
    global.staleness_window_seconds = staleness_window_seconds;
    global.max_concurrent_claims = max_concurrent_claims;
    global.challenge_bond_multiplier_bps = challenge_bond_multiplier as u16;
    global.bond_slash_treasury_bps = bond_slash_treasury_bps;
    global.paused = paused;
    global.paused_platforms = paused_platforms;
    // `min_escrow_lamports` is intentionally not written here. The slot is
    // preserved on the struct for binary compat (227 bytes) but no
    // instruction reads it after the 2026-05-07 removal.
    global.rate_limit_window_seconds = rate_limit_window_seconds;
    global.max_tasks_per_rate_window = max_tasks_per_rate_window;

    emit!(ParamsUpdated {
        protocol_fee_bps,
        quality_threshold,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct UpdateParams<'info> {
    #[account(
        mut,
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
}
