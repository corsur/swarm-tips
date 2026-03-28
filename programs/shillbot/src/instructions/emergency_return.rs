use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::EmergencyReturn;
use crate::state::{GlobalState, Task, TaskState};

/// Squads multisig only. Returns escrow for Open/Claimed tasks.
/// Accepts task accounts as remaining_accounts.
pub fn emergency_return(ctx: Context<EmergencyReturnAccounts>) -> Result<()> {
    let global = &ctx.accounts.global_state;

    // Checks: authority is the multisig
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Bounded iteration: limit remaining accounts
    require!(
        ctx.remaining_accounts.len() <= 20,
        ShillbotError::ArithmeticOverflow
    );

    let mut task_ids: Vec<u64> = Vec::new();

    // Process each task account in remaining_accounts.
    // We expect pairs: [task_account, client_account, task_account, client_account, ...]
    let accounts = ctx.remaining_accounts;
    let pair_count = accounts
        .len()
        .checked_div(2)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        accounts.len() % 2 == 0,
        ShillbotError::ArithmeticOverflow
    );

    let mut i: usize = 0;
    // Bounded: pair_count <= 10 (20/2)
    while i < pair_count {
        let idx = i
            .checked_mul(2)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        let task_info = &accounts[idx];
        let client_idx = idx
            .checked_add(1)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        let client_info = &accounts[client_idx];

        // Verify task is owned by this program
        require!(
            task_info.owner == ctx.program_id,
            ShillbotError::InvalidTaskState
        );

        let data = task_info.try_borrow_data()?;
        if data.len() >= Task::SPACE {
            let disc = &data[..8];
            let expected_disc = Task::DISCRIMINATOR;
            if disc == expected_disc {
                if let Ok(task) = Task::try_deserialize(&mut &data[..]) {
                    // Validate state: only Open or Claimed
                    require!(
                        task.state == TaskState::Open || task.state == TaskState::Claimed,
                        ShillbotError::InvalidTaskState
                    );

                    // Validate client matches
                    require!(
                        client_info.key() == task.client,
                        ShillbotError::NotTaskClient
                    );

                    task_ids.push(task.task_id);

                    // Must drop borrow before mutating lamports and data
                    drop(data);

                    // Close the task account: transfer ALL lamports (escrow + rent)
                    // to the client and zero the account data.
                    let all_lamports = task_info.lamports();
                    let client_lamports = client_info.lamports();

                    let new_client = client_lamports
                        .checked_add(all_lamports)
                        .ok_or(ShillbotError::ArithmeticOverflow)?;

                    **task_info.try_borrow_mut_lamports()? = 0;
                    **client_info.try_borrow_mut_lamports()? = new_client;

                    // Zero account data to mark it as closed
                    task_info.assign(&anchor_lang::system_program::ID);
                    let mut task_data = task_info.try_borrow_mut_data()?;
                    task_data.fill(0);
                } else {
                    drop(data);
                }
            } else {
                drop(data);
            }
        } else {
            drop(data);
        }

        i = i
            .checked_add(1)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
    }

    emit!(EmergencyReturn { task_ids });

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
