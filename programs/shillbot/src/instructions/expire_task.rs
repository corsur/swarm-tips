use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskExpired;
use crate::state::{AgentState, Task, TaskState};
use crate::VERIFICATION_TIMEOUT_SECONDS;

/// Permissionless crank: anyone can call after deadline (Open/Claimed) or
/// T+14d verification timeout (Submitted). Returns escrow to client.
///
/// When expiring a Claimed task, the agent's AgentState.claimed_count is
/// decremented. The agent_state account is optional — it is only required
/// when the task is in Claimed state.
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

    // Effects: if task was Claimed, decrement the agent's claim count.
    if state_at_expiry == TaskState::Claimed {
        decrement_agent_claim_count(ctx.remaining_accounts, ctx.program_id, &task.agent)?;
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
        seeds = [
            b"task",
            task.task_id.to_le_bytes().as_ref(),
            task.client.as_ref(),
        ],
        bump = task.bump,
    )]
    pub task: Account<'info, Task>,
    /// CHECK: Validated as task.client.
    #[account(
        mut,
        constraint = client.key() == task.client @ ShillbotError::NotTaskClient,
    )]
    pub client: AccountInfo<'info>,
    // For Claimed tasks, the agent's AgentState account must be passed
    // as the first remaining_account (mut) so claimed_count can be decremented.
}

/// Decrement the agent's concurrent claim count when a Claimed task expires.
/// The AgentState is passed as the first remaining_account.
fn decrement_agent_claim_count(
    remaining_accounts: &[AccountInfo],
    program_id: &Pubkey,
    expected_agent: &Pubkey,
) -> Result<()> {
    require!(
        !remaining_accounts.is_empty(),
        ShillbotError::MissingAgentState
    );
    let agent_state_info = &remaining_accounts[0];

    require!(
        agent_state_info.owner == program_id,
        ShillbotError::InvalidTaskState
    );

    let mut data = agent_state_info.try_borrow_mut_data()?;
    let mut agent_state = AgentState::try_deserialize(&mut &data[..])?;

    require!(
        agent_state.agent == *expected_agent,
        ShillbotError::NotTaskAgent
    );
    require!(
        agent_state.claimed_count > 0,
        ShillbotError::ArithmeticOverflow
    );

    agent_state.claimed_count = agent_state
        .claimed_count
        .checked_sub(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    agent_state.try_serialize(&mut &mut data[..])?;
    Ok(())
}
