use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::EmergencyReturn;
use crate::state::{GlobalState, Task, TaskState};
use crate::MAX_EMERGENCY_RETURN_ACCOUNTS;

/// Squads multisig only. Returns escrow for Open/Claimed tasks.
/// Accepts task accounts as remaining_accounts.
///
/// KNOWN LIMITATION: For Claimed tasks, the agent's AgentState.claimed_count is
/// NOT decremented here. Emergency return is a rare admin operation, and handling
/// the agent_state accounts in remaining_accounts alongside task/client pairs
/// adds significant complexity (triples would be needed: task, client, agent_state).
/// The practical impact is minimal: the agent's claimed_count may be higher than
/// actual, which is conservative (prevents over-claiming, not under-claiming).
/// If an agent is affected, they can wait for the AgentState to be corrected
/// by subsequent submit_work or expire_task calls on their other tasks.
pub fn emergency_return(ctx: Context<EmergencyReturnAccounts>) -> Result<()> {
    let global = &ctx.accounts.global_state;

    // Checks: authority is the multisig
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Bounded iteration: limit remaining accounts
    require!(
        ctx.remaining_accounts.len() <= MAX_EMERGENCY_RETURN_ACCOUNTS,
        ShillbotError::InvalidAccountPairs
    );

    // Process each task account in remaining_accounts.
    // We expect pairs: [task_account, client_account, task_account, client_account, ...]
    let accounts = ctx.remaining_accounts;
    let pair_count = accounts
        .len()
        .checked_div(2)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(accounts.len() % 2 == 0, ShillbotError::InvalidAccountPairs);

    let mut task_ids: Vec<u64> = Vec::with_capacity(pair_count);

    let mut i: usize = 0;
    // Bounded: pair_count <= 10 (20/2)
    while i < pair_count {
        let idx = i.checked_mul(2).ok_or(ShillbotError::ArithmeticOverflow)?;
        let task_info = &accounts[idx];
        let client_idx = idx
            .checked_add(1)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        let client_info = &accounts[client_idx];

        require!(
            task_info.owner == ctx.program_id,
            ShillbotError::InvalidTaskState
        );

        if let Some(task_id) = process_emergency_task(task_info, client_info, i)? {
            task_ids.push(task_id);
        }

        i = i.checked_add(1).ok_or(ShillbotError::ArithmeticOverflow)?;
    }

    emit!(EmergencyReturn { task_ids });

    Ok(())
}

/// Validate, close, and return escrow for a single task account.
/// Returns the task_id if processed, None if the account was skipped.
fn process_emergency_task(
    task_info: &AccountInfo,
    client_info: &AccountInfo,
    pair_index: usize,
) -> Result<Option<u64>> {
    let data = task_info.try_borrow_data()?;

    if data.len() < Task::SPACE {
        msg!(
            "emergency_return: skipped account at index {} (insufficient data)",
            pair_index
        );
        return Ok(None);
    }

    if &data[..8] != Task::DISCRIMINATOR {
        msg!(
            "emergency_return: skipped account at index {} (discriminator mismatch)",
            pair_index
        );
        return Ok(None);
    }

    let task = match Task::try_deserialize(&mut &data[..]) {
        Ok(t) => t,
        Err(_) => {
            msg!(
                "emergency_return: skipped account at index {} (deserialization failed)",
                pair_index
            );
            return Ok(None);
        }
    };

    require!(
        task.state == TaskState::Open || task.state == TaskState::Claimed,
        ShillbotError::InvalidTaskState
    );
    require!(
        client_info.key() == task.client,
        ShillbotError::NotTaskClient
    );

    let task_id = task.task_id;
    drop(data);

    // Close task: transfer ALL lamports to client, zero account data
    let all_lamports = task_info.lamports();
    let new_client = client_info
        .lamports()
        .checked_add(all_lamports)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    **task_info.try_borrow_mut_lamports()? = 0;
    **client_info.try_borrow_mut_lamports()? = new_client;

    task_info.assign(&anchor_lang::system_program::ID);
    task_info.try_borrow_mut_data()?.fill(0);

    Ok(Some(task_id))
}

#[derive(Accounts)]
pub struct EmergencyReturnAccounts<'info> {
    #[account(
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
}
