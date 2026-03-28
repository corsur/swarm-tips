use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskExpired;
use crate::state::{Task, TaskState};
use crate::VERIFICATION_TIMEOUT_SECONDS;

/// Permissionless crank: anyone can call after deadline (Open/Claimed) or
/// T+14d verification timeout (Submitted). Returns escrow to client.
pub fn expire_task(ctx: Context<ExpireTask>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;

    // Checks: valid expiry conditions
    let state_at_expiry = task.state;
    match state_at_expiry {
        TaskState::Open | TaskState::Claimed => {
            require!(
                clock.unix_timestamp > task.deadline,
                ShillbotError::DeadlineExpired
            );
        }
        TaskState::Submitted => {
            let verification_deadline = task
                .submitted_at
                .checked_add(VERIFICATION_TIMEOUT_SECONDS)
                .ok_or(ShillbotError::ArithmeticOverflow)?;
            require!(
                clock.unix_timestamp > verification_deadline,
                ShillbotError::VerificationTimeoutNotReached
            );
        }
        _ => {
            return Err(error!(ShillbotError::InvalidTaskState));
        }
    }

    let escrow = task.escrow_lamports;

    // Effects: no meaningful state change needed — account will be closed.
    // We do not set a new state since the account is about to be zeroed by `close`.
    let task = &mut ctx.accounts.task;

    // Interactions: return escrow to client via lamport transfer
    let task_info = task.to_account_info();
    let client_info = ctx.accounts.client.to_account_info();

    let task_lamports = task_info.lamports();
    let client_lamports = client_info.lamports();

    let new_task = task_lamports
        .checked_sub(escrow)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let new_client = client_lamports
        .checked_add(escrow)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    **task_info.try_borrow_mut_lamports()? = new_task;
    **client_info.try_borrow_mut_lamports()? = new_client;

    emit!(TaskExpired {
        task_id: task.task_id,
        state_at_expiry: state_at_expiry as u8,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct ExpireTask<'info> {
    #[account(
        mut,
        close = client,
    )]
    pub task: Account<'info, Task>,
    /// CHECK: Validated as task.client.
    #[account(
        mut,
        constraint = client.key() == task.client @ ShillbotError::NotTaskClient,
    )]
    pub client: AccountInfo<'info>,
}
