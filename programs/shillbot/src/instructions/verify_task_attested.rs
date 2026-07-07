use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskVerified;
use crate::scoring::compute_payment;
use crate::state::{GlobalState, Task, TaskState};

/// Attester-signed verification for `verification_kind = 1`
/// (DeterministicAttested) tasks — the deterministic-check path where an
/// allow-listed attester service (e.g. the pinned-Lean proof checker)
/// re-runs the check and its transaction signature releases payment.
///
/// Trust model vs `verify_task`: the Switchboard path is permissionless and
/// trusts the feed pubkey; THIS path requires the (rotatable)
/// `GlobalState.oracle_authority` as a transaction `Signer` — the first
/// instruction to enforce that field. `verification_hash` is the
/// `chain-core::verify_schema::AttestationCert` digest, binding
/// chain/program/task/statement/policy/artifact/score for the audit trail.
pub fn verify_task_attested(
    ctx: Context<VerifyTaskAttested>,
    score: u64,
    verification_hash: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let global = &ctx.accounts.global_state;
    // Checks
    validate_attested_inputs(
        task,
        global,
        &ctx.accounts.attester,
        score,
        verification_hash,
    )?;
    // Effects: payment+fee pinned at verification time (same rule as
    // verify_task / S-03); Verified transition + challenge deadline.
    let (payment_amount, fee_amount) = compute_payment(
        score,
        global.quality_threshold,
        task.escrow_lamports,
        global.protocol_fee_bps,
    )?;
    crate::instructions::verify_task::commit_verify_state(
        &mut ctx.accounts.task,
        global,
        score,
        verification_hash,
        payment_amount,
        fee_amount,
        clock.unix_timestamp,
    )?;
    // Interactions: none
    emit!(TaskVerified {
        task_id: ctx.accounts.task.task_id,
        composite_score: score,
        payment_amount,
        fee_amount,
        verification_hash,
    });
    Ok(())
}

/// Pure check phase — kind gate, attester allow-list, arms-length,
/// binary score, threshold sanity, required state, non-zero hash.
fn validate_attested_inputs(
    task: &Task,
    global: &GlobalState,
    attester: &Signer,
    score: u64,
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
    require!(
        task.verification_kind == 1,
        ShillbotError::VerificationKindMismatch
    );
    require!(
        attester.key() == global.oracle_authority,
        ShillbotError::OracleAuthorityMismatch
    );
    // Arms-length: the attester key releasing payment must not be the
    // worker being paid (wash-attestation guard).
    require!(
        global.oracle_authority != task.agent,
        ShillbotError::ArmsLengthViolation
    );
    require!(
        score == 0 || score == shared::MAX_SCORE,
        ShillbotError::AttestedScoreNotBinary
    );
    // Fail loud on the pays-zero parameter edge: threshold == MAX_SCORE
    // makes a passing binary score pay 0 silently (scoring.rs formula).
    require!(
        global.quality_threshold < shared::MAX_SCORE,
        ShillbotError::QualityThresholdTooHighForBinary
    );
    require!(
        verification_hash != [0u8; 32],
        ShillbotError::InvalidVerificationHash
    );
    Ok(())
}

#[derive(Accounts)]
pub struct VerifyTaskAttested<'info> {
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
    /// The allow-listed attester — must match `GlobalState.oracle_authority`
    /// (validated in the handler with a named error; rotate via
    /// `update_oracle_authority`).
    pub attester: Signer<'info>,
}
