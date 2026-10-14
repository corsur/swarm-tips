use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::EmergencyReturn;
use crate::state::{AgentState, GlobalState, Task, TaskState};
use crate::MAX_EMERGENCY_RETURN_ACCOUNTS;

/// Authority only. Returns escrow for Open/Claimed tasks.
/// Accepts `[task_account, client_account, ...]` pairs as `remaining_accounts`,
/// optionally followed by `AgentState` PDAs for any `Claimed` tasks so their
/// `claimed_count` is decremented in the same transaction (SC-MED-01).
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

    // Split trailing AgentState accounts from the leading [task, client] pairs.
    let accounts = ctx.remaining_accounts;
    let mut pair_end = accounts.len();
    while pair_end > 0 {
        let candidate = &accounts[pair_end.saturating_sub(1)];
        let is_agent_state = if candidate.owner == ctx.program_id {
            let data = candidate.try_borrow_data()?;
            data.len() >= 8 && &data[..8] == AgentState::DISCRIMINATOR
        } else {
            false
        };
        if is_agent_state {
            pair_end = pair_end.saturating_sub(1);
        } else {
            break;
        }
    }

    let pair_accounts = &accounts[..pair_end];
    require!(
        pair_accounts.len().is_multiple_of(2),
        ShillbotError::InvalidAccountPairs
    );
    let pair_count = pair_accounts
        .len()
        .checked_div(2)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    let mut task_ids: Vec<u64> = Vec::with_capacity(pair_count);

    let mut i: usize = 0;
    while i < pair_count {
        let idx = i.checked_mul(2).ok_or(ShillbotError::ArithmeticOverflow)?;
        let task_info = &pair_accounts[idx];
        let client_idx = idx
            .checked_add(1)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        let client_info = &pair_accounts[client_idx];

        require!(
            task_info.owner == ctx.program_id,
            ShillbotError::InvalidTaskState
        );

        if let Some((task_id, claimed_agent)) = process_emergency_task(task_info, client_info, i)? {
            if let Some(agent_pubkey) = claimed_agent {
                decrement_agent_claimed_count(accounts, ctx.program_id, &agent_pubkey)?;
            }
            task_ids.push(task_id);
        }

        i = i.checked_add(1).ok_or(ShillbotError::ArithmeticOverflow)?;
    }

    emit!(EmergencyReturn { task_ids });

    Ok(())
}

/// Validate, close, and return escrow for a single task account.
/// Returns `(task_id, Some(agent))` if a `Claimed` task was processed,
/// `(task_id, None)` if an `Open` task was processed, or `None` if skipped.
fn process_emergency_task(
    task_info: &AccountInfo,
    client_info: &AccountInfo,
    pair_index: usize,
) -> Result<Option<(u64, Option<Pubkey>)>> {
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
    let claimed_agent = if task.state == TaskState::Claimed {
        Some(task.agent)
    } else {
        None
    };
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

    Ok(Some((task_id, claimed_agent)))
}

/// Decrements `AgentState.claimed_count` when a `Claimed` task is emergency-returned
/// and the corresponding `AgentState` account is present in `remaining_accounts`.
fn decrement_agent_claimed_count(
    remaining_accounts: &[AccountInfo],
    program_id: &Pubkey,
    expected_agent: &Pubkey,
) -> Result<()> {
    for acc in remaining_accounts {
        if acc.owner != program_id {
            continue;
        }
        let mut data = acc.try_borrow_mut_data()?;
        if data.len() < 8 || &data[..8] != AgentState::DISCRIMINATOR {
            continue;
        }
        let mut agent_state = match AgentState::try_deserialize(&mut &data[..]) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if agent_state.agent != *expected_agent {
            continue;
        }
        agent_state.claimed_count = agent_state.claimed_count.saturating_sub(1);
        agent_state.try_serialize(&mut &mut data[..])?;
        return Ok(());
    }
    msg!(
        "emergency_return: AgentState for {} not provided in remaining_accounts; claimed_count unchanged",
        expected_agent
    );
    Ok(())
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
