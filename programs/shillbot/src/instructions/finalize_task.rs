use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskFinalized;
use crate::state::{AgentState, GlobalState, Task, TaskState};
use crate::transfers::transfer_lamports;

/// Permissionless crank: anyone can call after the challenge deadline passes.
/// Releases payment to agent, fee to treasury, remainder to client.
///
/// Uses the payment and fee amounts stored on the task at verification time,
/// rather than recomputing from current GlobalState parameters. This prevents
/// parameter-change bricking (S-03): if protocol_fee_bps or quality_threshold
/// change between verify and finalize, the stored values are still valid.
pub fn finalize_task(ctx: Context<FinalizeTask>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;

    // Checks: state
    require!(
        task.state == TaskState::Verified,
        ShillbotError::InvalidTaskState
    );

    // Checks: challenge window has closed
    require!(
        clock.unix_timestamp > task.challenge_deadline,
        ShillbotError::ChallengeWindowOpen
    );

    let payment_amount = task.payment_amount;
    let fee_amount = task.fee_amount;

    // Postcondition: payment + fee <= escrow
    let total_out = payment_amount
        .checked_add(fee_amount)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        total_out <= task.escrow_lamports,
        ShillbotError::PaymentExceedsEscrow
    );

    let remainder = task
        .escrow_lamports
        .checked_sub(total_out)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Effects
    let task = &mut ctx.accounts.task;
    task.state = TaskState::Finalized;

    // Interactions: distribute payment, fee, and remainder
    distribute_finalized_payment(
        &task.to_account_info(),
        &ctx.accounts.agent.to_account_info(),
        &ctx.accounts.treasury.to_account_info(),
        &ctx.accounts.client.to_account_info(),
        payment_amount,
        fee_amount,
        remainder,
    )?;

    // If AgentState is passed as remaining_account, update stats.
    // Reputation counters (total_completed, total_score_sum) only advance
    // when payment_amount > 0 (score >= quality_threshold) — preserves
    // pre-#12 behavior. See `state/agent.rs` doc-comment "Counter semantics".
    if payment_amount > 0 {
        update_agent_stats(
            ctx.remaining_accounts,
            ctx.program_id,
            &task.agent,
            payment_amount,
            task.composite_score,
        )?;
    }

    emit!(TaskFinalized {
        task_id: task.task_id,
        agent: task.agent,
        payment_amount,
        fee_amount,
    });

    Ok(())
}

/// If an AgentState account is passed as the first remaining_account, increment
/// `total_completed`, `total_earned`, and `total_score_sum`. This is optional
/// — callers that don't care about agent stats can omit it.
fn update_agent_stats(
    remaining_accounts: &[AccountInfo],
    program_id: &Pubkey,
    expected_agent: &Pubkey,
    payment_amount: u64,
    composite_score: u64,
) -> Result<()> {
    if remaining_accounts.is_empty() {
        return Ok(());
    }
    let agent_state_info = &remaining_accounts[0];

    // Validate ownership
    if agent_state_info.owner != program_id {
        return Ok(());
    }

    let mut data = agent_state_info.try_borrow_mut_data()?;
    let mut agent_state = match AgentState::try_deserialize(&mut &data[..]) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };

    // Validate agent matches
    if agent_state.agent != *expected_agent {
        return Ok(());
    }

    agent_state.total_completed = agent_state
        .total_completed
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    agent_state.total_earned = agent_state
        .total_earned
        .checked_add(payment_amount)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    agent_state.total_score_sum = agent_state
        .total_score_sum
        .checked_add(composite_score)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    agent_state.try_serialize(&mut &mut data[..])?;
    Ok(())
}

/// Transfer payment to agent, fee to treasury, remainder to client.
/// Asserts that total distributed does not exceed the task account's lamport balance.
fn distribute_finalized_payment(
    task_info: &AccountInfo,
    agent_info: &AccountInfo,
    treasury_info: &AccountInfo,
    client_info: &AccountInfo,
    payment: u64,
    fee: u64,
    remainder: u64,
) -> Result<()> {
    // Precondition: total distributed <= task account lamports
    let total_distributed = payment
        .checked_add(fee)
        .ok_or(ShillbotError::ArithmeticOverflow)?
        .checked_add(remainder)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        total_distributed <= task_info.lamports(),
        ShillbotError::PaymentExceedsEscrow
    );

    if payment > 0 {
        transfer_lamports(task_info, agent_info, payment)?;
    }
    if fee > 0 {
        transfer_lamports(task_info, treasury_info, fee)?;
    }
    if remainder > 0 {
        transfer_lamports(task_info, client_info, remainder)?;
    }
    Ok(())
}

#[derive(Accounts)]
pub struct FinalizeTask<'info> {
    #[account(
        mut,
        close = client,
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
    /// CHECK: Validated as task.agent in handler.
    #[account(
        mut,
        constraint = agent.key() == task.agent @ ShillbotError::NotTaskAgent,
    )]
    pub agent: AccountInfo<'info>,
    /// CHECK: Validated as task.client.
    #[account(
        mut,
        constraint = client.key() == task.client @ ShillbotError::NotTaskClient,
    )]
    pub client: AccountInfo<'info>,
    /// CHECK: Treasury account for protocol fees. Validated against GlobalState.treasury.
    #[account(
        mut,
        constraint = treasury.key() == global_state.treasury @ ShillbotError::NotAuthority,
    )]
    pub treasury: AccountInfo<'info>,
}
