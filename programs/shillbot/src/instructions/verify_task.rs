use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskVerified;
use crate::scoring::compute_payment;
use crate::state::{GlobalState, Task, TaskState};

/// Oracle attestation records the composite score and computes payment.
///
/// The authority account represents the Switchboard feed attestation signer.
/// Immutable invariant: the feed PDA must be derived from fixed seeds and owned
/// by the Switchboard program. For devnet, the oracle_authority in GlobalState signs directly.
pub fn verify_task(ctx: Context<VerifyTask>, composite_score: u64) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let global = &ctx.accounts.global_state;

    // Checks: state
    require!(
        task.state == TaskState::Submitted,
        ShillbotError::InvalidTaskState
    );

    // Checks: oracle authority (separate from protocol authority)
    require!(
        ctx.accounts.authority.key() == global.oracle_authority,
        ShillbotError::OracleAuthorityMismatch
    );

    // Checks: score bounds
    require!(
        composite_score <= shared::MAX_SCORE,
        ShillbotError::ScoreOutOfBounds
    );

    // Checks: staleness — attestation within staleness_window of submitted_at + attestation_delay
    // (the oracle should attest around T+attestation_delay, but we allow a window)
    let expected_attestation_time = task
        .submitted_at
        .checked_add(global.attestation_delay_seconds)
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
    task.challenge_deadline = clock
        .unix_timestamp
        .checked_add(global.challenge_window_seconds)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    task.state = TaskState::Verified;

    // Interactions: none
    emit!(TaskVerified {
        task_id: task.task_id,
        composite_score,
        payment_amount,
        fee_amount,
    });

    Ok(())
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
    pub authority: Signer<'info>,
}
