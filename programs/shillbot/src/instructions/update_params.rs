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
    // Dispute-resolution liveness window (2026-07-07). 0 = disabled
    // (legacy: authority-only resolution); nonzero must fall in
    // [MIN, MAX]_DISPUTE_RESOLUTION_WINDOW_SECONDS.
    dispute_resolution_window_seconds: i64,
) -> Result<()> {
    let global = &ctx.accounts.global_state;
    // Checks: caller is authority
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );
    // Checks: every per-field bound on the new parameter set.
    validate_update(
        protocol_fee_bps,
        quality_threshold,
        challenge_window_seconds,
        verification_timeout_seconds,
        attestation_delay_seconds,
        staleness_window_seconds,
        max_concurrent_claims,
        challenge_bond_multiplier,
        bond_slash_treasury_bps,
        rate_limit_window_seconds,
        max_tasks_per_rate_window,
        dispute_resolution_window_seconds,
    )?;
    // Effects: commit the new parameter set.
    apply_update(
        &mut ctx.accounts.global_state,
        protocol_fee_bps,
        quality_threshold,
        challenge_window_seconds,
        verification_timeout_seconds,
        attestation_delay_seconds,
        staleness_window_seconds,
        max_concurrent_claims,
        challenge_bond_multiplier,
        bond_slash_treasury_bps,
        paused,
        paused_platforms,
        rate_limit_window_seconds,
        max_tasks_per_rate_window,
        dispute_resolution_window_seconds,
    );
    emit!(ParamsUpdated {
        protocol_fee_bps,
        quality_threshold,
    });
    Ok(())
}

/// Pure assignment of the validated parameter set to the GlobalState PDA.
#[allow(clippy::too_many_arguments)]
fn apply_update(
    global: &mut GlobalState,
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
    rate_limit_window_seconds: i64,
    max_tasks_per_rate_window: u32,
    dispute_resolution_window_seconds: i64,
) {
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
    global.dispute_resolution_window_seconds = dispute_resolution_window_seconds;
}

/// Validate every per-field bound on the parameter set passed to
/// `update_params`. Pure check phase — no I/O, no mutation.
#[allow(clippy::too_many_arguments)]
fn validate_update(
    protocol_fee_bps: u16,
    quality_threshold: u64,
    challenge_window_seconds: i64,
    verification_timeout_seconds: i64,
    attestation_delay_seconds: i64,
    staleness_window_seconds: i64,
    max_concurrent_claims: u8,
    challenge_bond_multiplier: u8,
    bond_slash_treasury_bps: u16,
    rate_limit_window_seconds: i64,
    max_tasks_per_rate_window: u32,
    dispute_resolution_window_seconds: i64,
) -> Result<()> {
    // Fee within bounds [100, 2500] bps
    require!(
        (shared::MIN_PROTOCOL_FEE_BPS..=shared::MAX_PROTOCOL_FEE_BPS).contains(&protocol_fee_bps),
        ShillbotError::ProtocolFeeBoundsExceeded
    );
    // Quality threshold within bounds
    require!(
        quality_threshold <= shared::MAX_SCORE,
        ShillbotError::QualityThresholdBoundsExceeded
    );
    // Timing parameters must be strictly positive
    require!(
        challenge_window_seconds > 0,
        ShillbotError::InvalidParameter
    );
    require!(
        verification_timeout_seconds > 0,
        ShillbotError::InvalidParameter
    );
    require!(
        attestation_delay_seconds > 0,
        ShillbotError::InvalidParameter
    );
    require!(
        staleness_window_seconds > 0,
        ShillbotError::InvalidParameter
    );
    require!(max_concurrent_claims > 0, ShillbotError::InvalidParameter);
    // Challenge bond multiplier within bounds [2, 10]
    require!(
        (MIN_CHALLENGE_BOND_MULTIPLIER..=MAX_CHALLENGE_BOND_MULTIPLIER)
            .contains(&challenge_bond_multiplier),
        ShillbotError::InsufficientBond
    );
    // bond_slash_treasury_bps within [0, 10000]
    require!(
        bond_slash_treasury_bps <= 10_000,
        ShillbotError::ProtocolFeeBoundsExceeded
    );
    // (D3) rate-limit window within bounds
    require!(
        (MIN_RATE_LIMIT_WINDOW_SECONDS..=MAX_RATE_LIMIT_WINDOW_SECONDS)
            .contains(&rate_limit_window_seconds),
        ShillbotError::RateLimitExceeded
    );
    // (D3) max_tasks_per_rate_window within bounds
    require!(
        (MIN_TASKS_PER_RATE_WINDOW..=MAX_TASKS_PER_RATE_WINDOW_CEILING)
            .contains(&max_tasks_per_rate_window),
        ShillbotError::RateLimitExceeded
    );
    // Dispute-resolution window: 0 = disabled, else within [1h, 30d].
    if dispute_resolution_window_seconds != 0 {
        require!(
            (crate::MIN_DISPUTE_RESOLUTION_WINDOW_SECONDS
                ..=crate::MAX_DISPUTE_RESOLUTION_WINDOW_SECONDS)
                .contains(&dispute_resolution_window_seconds),
            ShillbotError::InvalidParameter
        );
    }
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
