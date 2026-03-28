use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskFinalized;
use crate::scoring::compute_payment;
use crate::state::{GlobalState, Task, TaskState};

/// Permissionless crank: anyone can call after the challenge deadline passes.
/// Releases payment to agent, fee to treasury, remainder to client.
pub fn finalize_task(ctx: Context<FinalizeTask>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let global = &ctx.accounts.global_state;

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

    // Recompute payment to get fee breakdown
    let (payment_amount, fee_amount) = compute_payment(
        task.composite_score,
        global.quality_threshold,
        task.escrow_lamports,
        global.protocol_fee_bps,
    )?;

    // Postcondition: payment matches stored value
    require!(
        payment_amount == task.payment_amount,
        ShillbotError::ArithmeticOverflow
    );

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

    // Interactions: lamport transfers from the task PDA
    let task_info = task.to_account_info();

    if payment_amount > 0 {
        transfer_lamports_from_pda(
            &task_info,
            &ctx.accounts.agent.to_account_info(),
            payment_amount,
        )?;
    }
    if fee_amount > 0 {
        transfer_lamports_from_pda(
            &task_info,
            &ctx.accounts.treasury.to_account_info(),
            fee_amount,
        )?;
    }
    if remainder > 0 {
        transfer_lamports_from_pda(
            &task_info,
            &ctx.accounts.client.to_account_info(),
            remainder,
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

/// Transfer lamports from a PDA by directly adjusting lamport balances.
/// This is safe because the PDA is owned by this program.
fn transfer_lamports_from_pda(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<()> {
    let from_lamports = from.lamports();
    let to_lamports = to.lamports();

    let new_from = from_lamports
        .checked_sub(amount)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let new_to = to_lamports
        .checked_add(amount)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    **from.try_borrow_mut_lamports()? = new_from;
    **to.try_borrow_mut_lamports()? = new_to;

    Ok(())
}

#[derive(Accounts)]
pub struct FinalizeTask<'info> {
    #[account(
        mut,
        close = client,
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
