use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskVerified;
use crate::scoring::compute_payment;
use crate::state::{GlobalState, Task, TaskState};
use crate::{CHALLENGE_WINDOW_SECONDS, SEVEN_DAYS_SECONDS, STALENESS_WINDOW_SECONDS};

/// Oracle attestation records the composite score and computes payment.
///
/// The authority account represents the Switchboard feed attestation signer.
/// Immutable invariant: the feed PDA must be derived from fixed seeds and owned
/// by the Switchboard program. For devnet, the authority in GlobalState signs directly.
pub fn verify_task(ctx: Context<VerifyTask>, composite_score: u64) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let global = &ctx.accounts.global_state;

    // Checks: state
    require!(
        task.state == TaskState::Submitted,
        ShillbotError::InvalidTaskState
    );

    // Checks: authority (devnet: EOA authority; mainnet: Switchboard feed signer)
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::InvalidAttestation
    );

    // Checks: score bounds
    require!(
        composite_score <= shared::MAX_SCORE,
        ShillbotError::ScoreOutOfBounds
    );

    // Checks: staleness — attestation within STALENESS_WINDOW of submitted_at + 7 days
    // (the oracle should attest around T+7d, but we allow a window)
    let expected_attestation_time = task
        .submitted_at
        .checked_add(SEVEN_DAYS_SECONDS)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let earliest = expected_attestation_time
        .checked_sub(STALENESS_WINDOW_SECONDS)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let latest = expected_attestation_time
        .checked_add(STALENESS_WINDOW_SECONDS)
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
        .checked_add(CHALLENGE_WINDOW_SECONDS)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    task.state = TaskState::Verified;

    // Interactions: none
    emit!(TaskVerified {
        task_id: task.task_id,
        composite_score,
        payment_amount,
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
