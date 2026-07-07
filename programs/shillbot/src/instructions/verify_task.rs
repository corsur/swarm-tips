use anchor_lang::prelude::*;
use switchboard_on_demand::on_demand::accounts::pull_feed::PullFeedAccountData;
use switchboard_on_demand::prelude::rust_decimal::prelude::ToPrimitive;

use crate::errors::ShillbotError;
use crate::events::TaskVerified;
use crate::scoring::compute_payment;
use crate::state::{GlobalState, Task, TaskState};

// Switchboard On-Demand V3: get_value() returns a Decimal in human-readable
// form (no manual rescaling needed). Old V2 used i128 scaled by 10^18.

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
    // Checks
    validate_verify_inputs(task, composite_score, verification_hash)?;
    validate_switchboard_feed(global, &ctx.accounts.switchboard_feed)?;
    validate_attestation_staleness(task, global, clock.unix_timestamp)?;
    let feed_score = read_switchboard_score(&ctx.accounts.switchboard_feed, clock.slot)?;
    require!(
        feed_score == composite_score,
        ShillbotError::SwitchboardScoreMismatch
    );
    // Effects: payment+fee pinned at verification time so finalize/resolve
    // use the fee that was in effect now, not the current GlobalState fee.
    let (payment_amount, fee_amount) = compute_payment_amounts(task, global, composite_score)?;
    commit_verify_state(
        &mut ctx.accounts.task,
        global,
        composite_score,
        verification_hash,
        payment_amount,
        fee_amount,
        clock.unix_timestamp,
    )?;
    // Interactions: none
    emit!(TaskVerified {
        task_id: ctx.accounts.task.task_id,
        composite_score,
        payment_amount,
        fee_amount,
        verification_hash,
    });
    Ok(())
}

/// Pure check phase — required state, score bounds, non-zero hash.
fn validate_verify_inputs(
    task: &Task,
    composite_score: u64,
    verification_hash: [u8; 32],
) -> Result<()> {
    let required_state = if task.requires_approval == 1 {
        TaskState::Approved
    } else {
        TaskState::Submitted
    };
    require!(
        task.state == required_state,
        ShillbotError::InvalidTaskState
    );
    // Mutual exclusion with `verify_task_attested`: the Switchboard path
    // only admits kind 0 (OracleMetrics) tasks.
    require!(
        task.verification_kind == 0,
        ShillbotError::VerificationKindMismatch
    );
    require!(
        composite_score <= shared::MAX_SCORE,
        ShillbotError::ScoreOutOfBounds
    );
    require!(
        verification_hash != [0u8; 32],
        ShillbotError::InvalidVerificationHash
    );
    Ok(())
}

/// Pure effect phase — write verification fields and the Verified
/// transition. Resolves the per-task `challenge_window` override.
/// Shared with `verify_task_attested` so both verify entries pin
/// payment/fee and arm the challenge window identically.
pub(crate) fn commit_verify_state(
    task: &mut Task,
    global: &GlobalState,
    composite_score: u64,
    verification_hash: [u8; 32],
    payment_amount: u64,
    fee_amount: u64,
    now: i64,
) -> Result<()> {
    task.composite_score = composite_score;
    task.payment_amount = payment_amount;
    task.fee_amount = fee_amount;
    task.verified_at = now;
    task.verification_hash = verification_hash;
    let challenge_window = if task.challenge_window_override > 0 {
        i64::from(task.challenge_window_override)
    } else {
        global.challenge_window_seconds
    };
    task.challenge_deadline = now
        .checked_add(challenge_window)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    task.state = TaskState::Verified;
    Ok(())
}

/// Verify the Switchboard feed account passed by the caller matches the
/// one configured in GlobalState.
///
/// The 2026-05-01 commit (task #9) moved this check to a compile-time
/// const for hardening, on the theory that "single-key compromise of the
/// authority lets an attacker redirect verify to a malicious feed." That
/// hardening was reverted 2026-05-08 because (a) the const was committed
/// as a placeholder and the load-bearing TODO was missed, blocking every
/// verify_task call on every network for ~7 days, and (b) at zero TVL
/// the attacker-model was hypothetical, while the GlobalState field
/// already has correct per-network values populated.
fn validate_switchboard_feed(global: &GlobalState, feed: &AccountInfo) -> Result<()> {
    require!(
        global.switchboard_feed != Pubkey::default(),
        ShillbotError::SwitchboardFeedNotConfigured
    );
    require!(
        feed.key() == global.switchboard_feed,
        ShillbotError::SwitchboardFeedMismatch
    );
    Ok(())
}

/// Compute (payment_amount, fee_amount) for the verification using the
/// current GlobalState protocol_fee_bps and quality_threshold.
fn compute_payment_amounts(
    task: &Task,
    global: &GlobalState,
    composite_score: u64,
) -> Result<(u64, u64)> {
    compute_payment(
        composite_score,
        global.quality_threshold,
        task.escrow_lamports,
        global.protocol_fee_bps,
    )
}

/// Validate that the current timestamp falls within the acceptable staleness window
/// around the expected attestation time (submitted_at + attestation_delay).
///
/// Uses per-task override if nonzero, else global default for attestation delay.
fn validate_attestation_staleness(task: &Task, global: &GlobalState, now: i64) -> Result<()> {
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

    // Precondition: window bounds are coherent
    require!(earliest <= latest, ShillbotError::ArithmeticOverflow);

    require!(
        now >= earliest && now <= latest,
        ShillbotError::AttestationStale
    );

    Ok(())
}

/// Parse the Switchboard pull feed account and extract the composite score as a u64.
///
/// Switchboard On-Demand V3 `get_value()` returns a `Decimal` in human-readable
/// form (e.g., score 6600 is returned as `Decimal(6600)`). No rescaling needed.
///
/// This function:
/// 1. Parses the feed account data (validates discriminator)
/// 2. Reads the latest value with staleness and sample checks
/// 3. Converts the Decimal to u64
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

    // Switchboard On-Demand V3 get_value() returns a Decimal in human-readable
    // form (e.g., 6600 for composite score 6600). Convert directly to u64.
    let score = value
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

#[cfg(test)]
mod tests {
    /// Regression guard for the 2026-05-01 → 2026-05-08 const-lockdown
    /// round-trip: the `SWITCHBOARD_FEED` compile-time const is fully
    /// retired. The 2026-05-01 lockdown shipped a placeholder pubkey
    /// (`11111…112`) without filling in a real value, blocking every
    /// `verify_task` call on every network until the revert. This test
    /// fails-loudly if the const is reintroduced.
    #[test]
    fn switchboard_feed_const_is_not_referenced() {
        let src = include_str!("verify_task.rs");
        // Search only the non-test portion of the file.
        let cutoff = src
            .find("#[cfg(test)]")
            .expect("test module marker should exist");
        let production = &src[..cutoff];
        assert!(
            !production.contains("crate::constants::SWITCHBOARD_FEED"),
            "verify_task.rs must not reference crate::constants::SWITCHBOARD_FEED \
             (const retired 2026-05-08; per-network feed pubkey lives in \
             GlobalState.switchboard_feed). Re-introducing the const is the \
             same trap that shipped the placeholder bug."
        );
    }
}
