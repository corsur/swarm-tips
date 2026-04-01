use anchor_lang::prelude::*;
use switchboard_on_demand::on_demand::accounts::pull_feed::PullFeedAccountData;
use switchboard_on_demand::prelude::rust_decimal::prelude::ToPrimitive;

use crate::errors::ShillbotError;
use crate::events::TaskVerified;
use crate::scoring::compute_payment;
use crate::state::{GlobalState, Task, TaskState};

/// Switchboard oracle precision: values are i128 scaled by 10^18.
const SWITCHBOARD_PRECISION: u32 = 18;

/// Oracle attestation records the composite score from a Switchboard pull feed
/// and computes payment.
///
/// The off-chain flow:
/// 1. Off-chain verifier triggers a Switchboard feed update with the composite score
/// 2. Switchboard oracles post the score to the feed account
/// 3. This instruction reads the feed, validates the score, and records verification
///
/// The `composite_score` argument must match the value read from the Switchboard feed.
/// The `verification_hash` is computed off-chain and stored for audit trail.
pub fn verify_task(
    ctx: Context<VerifyTask>,
    composite_score: u64,
    verification_hash: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let global = &ctx.accounts.global_state;

    // Checks: state
    require!(
        task.state == TaskState::Submitted,
        ShillbotError::InvalidTaskState
    );

    // Checks: Switchboard feed is configured
    require!(
        global.switchboard_feed != Pubkey::default(),
        ShillbotError::SwitchboardFeedNotConfigured
    );

    // Checks: feed account matches the configured feed
    require!(
        ctx.accounts.switchboard_feed.key() == global.switchboard_feed,
        ShillbotError::SwitchboardFeedMismatch
    );

    // Checks: score bounds
    require!(
        composite_score <= shared::MAX_SCORE,
        ShillbotError::ScoreOutOfBounds
    );

    // Checks: verification hash must not be zero
    require!(
        verification_hash != [0u8; 32],
        ShillbotError::InvalidVerificationHash
    );

    // Checks: staleness — attestation within staleness_window of submitted_at + attestation_delay
    // Use per-task override if nonzero, else global default.
    let attestation_delay = if task.attestation_delay_override > 0 {
        i64::from(task.attestation_delay_override)
    } else {
        global.attestation_delay_seconds
    };
    let expected_attestation_time = task
        .submitted_at
        .checked_add(attestation_delay)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let earliest = expected_attestation_time
        .checked_sub(global.staleness_window_seconds)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let latest = expected_attestation_time
        .checked_add(global.staleness_window_seconds)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        clock.unix_timestamp >= earliest && clock.unix_timestamp <= latest,
        ShillbotError::AttestationStale
    );

    // Checks: parse Switchboard feed and validate composite_score matches the oracle value
    let feed_value = read_switchboard_score(&ctx.accounts.switchboard_feed, clock.slot)?;
    require!(
        feed_value == composite_score,
        ShillbotError::SwitchboardScoreMismatch
    );

    // Compute payment and fee — stored on the task so finalize/resolve use the
    // fee that was in effect at verification time, not the current GlobalState fee.
    let (payment_amount, fee_amount) = compute_payment(
        composite_score,
        global.quality_threshold,
        task.escrow_lamports,
        global.protocol_fee_bps,
    )?;

    // Effects
    let task = &mut ctx.accounts.task;
    task.composite_score = composite_score;
    task.payment_amount = payment_amount;
    task.fee_amount = fee_amount;
    task.verified_at = clock.unix_timestamp;
    task.verification_hash = verification_hash;
    let challenge_window = if task.challenge_window_override > 0 {
        i64::from(task.challenge_window_override)
    } else {
        global.challenge_window_seconds
    };
    task.challenge_deadline = clock
        .unix_timestamp
        .checked_add(challenge_window)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    task.state = TaskState::Verified;

    // Interactions: none
    emit!(TaskVerified {
        task_id: task.task_id,
        composite_score,
        payment_amount,
        fee_amount,
        verification_hash,
    });

    Ok(())
}

/// Parse the Switchboard pull feed account and extract the composite score as a u64.
///
/// The Switchboard feed stores values as i128 scaled by 10^18 (PRECISION = 18).
/// Our composite scores are u64 values in range [0, MAX_SCORE] (MAX_SCORE = 1_000_000).
/// The oracle function posts the composite score directly (e.g., score 500_000 is posted
/// as 500_000 * 10^18 in the feed).
///
/// This function:
/// 1. Parses the feed account data (validates discriminator)
/// 2. Reads the latest value with staleness and sample checks
/// 3. Converts from Decimal (10^18 scale) to u64
fn read_switchboard_score(feed_account: &AccountInfo, clock_slot: u64) -> Result<u64> {
    let data = feed_account
        .try_borrow_data()
        .map_err(|_| error!(ShillbotError::SwitchboardParseError))?;

    let feed = PullFeedAccountData::parse(data)
        .map_err(|_| error!(ShillbotError::SwitchboardParseError))?;

    // Read value with staleness check (use the feed's own max_staleness config),
    // require at least 1 sample, and require positive value.
    let value = feed
        .get_value(clock_slot, feed.max_staleness as u64, 1, true)
        .map_err(|_| error!(ShillbotError::SwitchboardParseError))?;

    // Rescale from 10^18 precision to integer.
    // The oracle posts the composite score as an integer value, so after
    // removing the 10^18 scale we should get a clean integer.
    let rescaled = value
        .checked_div(switchboard_on_demand::prelude::rust_decimal::Decimal::from_i128_with_scale(
            1,
            SWITCHBOARD_PRECISION,
        ))
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Convert to u64 — value must be non-negative and fit in u64
    let score = rescaled
        .to_u64()
        .ok_or(ShillbotError::SwitchboardInvalidValue)?;

    // Postcondition: score within valid bounds
    require!(score <= shared::MAX_SCORE, ShillbotError::ScoreOutOfBounds);

    Ok(score)
}

#[derive(Accounts)]
pub struct VerifyTask<'info> {
    #[account(
        mut,
        seeds = [
            b"task",
            task.task_id.to_le_bytes().as_ref(),
            task.client.as_ref(),
        ],
        bump = task.bump,
    )]
    pub task: Account<'info, Task>,
    #[account(
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    /// The Switchboard pull feed account containing the oracle-attested composite score.
    /// CHECK: Validated against `global_state.switchboard_feed` and parsed via
    /// `PullFeedAccountData::parse()` which checks the account discriminator.
    /// Account ownership is verified by Switchboard's `Owner` implementation.
    pub switchboard_feed: AccountInfo<'info>,
}
