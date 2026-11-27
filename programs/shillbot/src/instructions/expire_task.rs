use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskExpired;
use crate::state::{AgentState, GlobalState, Task, TaskState};

/// Permissionless crank: anyone can call after deadline (Open/Claimed) or
/// T+verification_timeout verification timeout (Submitted). Returns escrow to client.
///
/// When expiring a Claimed task, the agent's AgentState.claimed_count is
/// decremented. The agent_state account is optional — it is only required
/// when the task is in Claimed state.
pub fn expire_task(ctx: Context<ExpireTask>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let global = &ctx.accounts.global_state;
    let state_at_expiry = task.state;

    // Checks
    validate_expiry_eligibility(task, global, state_at_expiry, clock.unix_timestamp)?;

    // Effects: if task was Claimed, decrement the agent's claim count.
    if state_at_expiry == TaskState::Claimed {
        decrement_agent_claim_count(ctx.remaining_accounts, ctx.program_id, &task.agent)?;
    }

    let escrow = task.escrow_lamports;
    let task_id = task.task_id;
    let platform = task.platform;
    // No state change to commit — the account is about to be closed.

    // Interactions: return escrow to client via lamport transfer.
    return_escrow(
        &ctx.accounts.task.to_account_info(),
        &ctx.accounts.client.to_account_info(),
        escrow,
    )?;

    emit!(TaskExpired {
        task_id,
        state_at_expiry: state_at_expiry as u8,
        platform,
    });

    Ok(())
}

/// Verify that the task is in a state that allows expiry, and that enough
/// time has elapsed for the relevant deadline.
fn validate_expiry_eligibility(
    task: &Task,
    global: &GlobalState,
    state_at_expiry: TaskState,
    now: i64,
) -> Result<()> {
    match state_at_expiry {
        TaskState::Open | TaskState::Claimed => {
            require!(now > task.deadline, ShillbotError::DeadlineExpired);
        }
        TaskState::Submitted | TaskState::Approved => {
            // Phase 3 blocker #3a: Approved is a new state inserted between
            // Submitted and Verified. The verification timeout is measured
            // from `submitted_at` for both — client approval does not reset
            // the clock.
            let verification_timeout = if task.verification_timeout_override > 0 {
                i64::from(task.verification_timeout_override)
            } else {
                global.verification_timeout_seconds
            };
            let verification_deadline = task
                .submitted_at
                .checked_add(verification_timeout)
                .ok_or(ShillbotError::ArithmeticOverflow)?;
            require!(
                now > verification_deadline,
                ShillbotError::VerificationTimeoutNotReached
            );
        }
        _ => return Err(error!(ShillbotError::InvalidTaskState)),
    }
    Ok(())
}

/// Return the escrow lamports from the task PDA to the client wallet.
fn return_escrow(task_info: &AccountInfo, client_info: &AccountInfo, escrow: u64) -> Result<()> {
    let new_task = task_info
        .lamports()
        .checked_sub(escrow)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let new_client = client_info
        .lamports()
        .checked_add(escrow)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    **task_info.try_borrow_mut_lamports()? = new_task;
    **client_info.try_borrow_mut_lamports()? = new_client;
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
    #[account(
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
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
    decrement_claimed_count_saturating(&mut agent_state);

    agent_state.try_serialize(&mut &mut data[..])?;
    Ok(())
}

/// Saturating decrement (matching `emergency_return`) so any historical
/// `AgentState` whose `claimed_count` was previously undercounted before P1-02
/// can never permanently strand a client's escrow on `expire_task` (SF3).
pub(crate) fn decrement_claimed_count_saturating(agent_state: &mut AgentState) {
    agent_state.claimed_count = agent_state.claimed_count.saturating_sub(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expire_task_decrement_saturates_at_zero_for_legacy_undercount() {
        let mut state = AgentState {
            agent: Pubkey::new_unique(),
            claimed_count: 2,
            total_completed: 0,
            total_earned: 0,
            total_score_sum: 0,
            total_tasks_claimed: 2,
            total_challenges_lost: 0,
            last_strike_at: 0,
            bump: 255,
        };

        decrement_claimed_count_saturating(&mut state);
        assert_eq!(state.claimed_count, 1);

        decrement_claimed_count_saturating(&mut state);
        assert_eq!(state.claimed_count, 0);

        // Pre-P1-02 undercounted account at 0 must saturate instead of stranding escrow.
        decrement_claimed_count_saturating(&mut state);
        assert_eq!(state.claimed_count, 0);
    }
}
