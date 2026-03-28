use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::errors::ShillbotError;
use crate::events::TaskChallenged;
use crate::scoring::compute_challenge_bond;
use crate::state::{Challenge, Task, TaskState};
use crate::{FREE_CHALLENGE_PERCENT, MIN_CHALLENGE_BOND_MULTIPLIER};

/// Anyone can challenge a verified task by posting a bond during the challenge window.
/// Client exception: the task's client gets 20% free challenges on the campaign.
///
/// KNOWN LIMITATION (v1/devnet): `client_challenges` is tracked per-Task, not per-Campaign.
/// This means the free challenge counter resets with each new task, so a client effectively
/// gets unlimited free challenges (one per task). The correct fix requires either:
/// (a) a Campaign PDA with a shared challenge counter across all tasks in the campaign, or
/// (b) removing the free challenge feature entirely for v1.
/// The `total_campaign_tasks` parameter is passed by the caller and not verified on-chain,
/// which compounds the issue. This is acceptable for devnet but must be resolved before
/// mainnet deployment. See smartcontracts/CLAUDE.md Open Questions.
pub fn challenge_task(ctx: Context<ChallengeTask>, total_campaign_tasks: u16) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let challenger_key = ctx.accounts.challenger.key();

    // Checks: state
    require!(
        task.state == TaskState::Verified,
        ShillbotError::InvalidTaskState
    );

    // Checks: within challenge window
    require!(
        clock.unix_timestamp < task.challenge_deadline,
        ShillbotError::ChallengeWindowClosed
    );

    // Determine bond amount
    let is_client_challenge = challenger_key == task.client;
    let bond_lamports: u64;

    if is_client_challenge {
        // Client free challenge check: 20% of campaign tasks
        let free_challenge_limit = (total_campaign_tasks as u64)
            .checked_mul(FREE_CHALLENGE_PERCENT as u64)
            .ok_or(ShillbotError::ArithmeticOverflow)?
            .checked_div(100)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        let free_limit_u16 = u16::try_from(free_challenge_limit)
            .map_err(|_| error!(ShillbotError::ArithmeticOverflow))?;

        if task.client_challenges < free_limit_u16 {
            bond_lamports = 0;
        } else {
            bond_lamports =
                compute_challenge_bond(task.escrow_lamports, MIN_CHALLENGE_BOND_MULTIPLIER)?;
        }
    } else {
        bond_lamports =
            compute_challenge_bond(task.escrow_lamports, MIN_CHALLENGE_BOND_MULTIPLIER)?;
    }

    // Effects: initialize challenge
    let challenge = &mut ctx.accounts.challenge;
    challenge.task_id = task.task_id;
    challenge.challenger = challenger_key;
    challenge.bond_lamports = bond_lamports;
    challenge.is_client_challenge = is_client_challenge;
    challenge.created_at = clock.unix_timestamp;
    challenge.resolved = false;
    challenge.challenger_won = false;
    challenge.bump = ctx.bumps.challenge;

    // Effects: update task
    let task = &mut ctx.accounts.task;
    task.state = TaskState::Disputed;
    // Only increment the free challenge counter when the client actually used a free challenge
    if is_client_challenge && bond_lamports == 0 {
        task.client_challenges = task
            .client_challenges
            .checked_add(1)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
    }

    // Interactions: transfer bond from challenger to challenge PDA
    if bond_lamports > 0 {
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.challenger.to_account_info(),
                    to: ctx.accounts.challenge.to_account_info(),
                },
            ),
            bond_lamports,
        )?;
    }

    emit!(TaskChallenged {
        task_id: task.task_id,
        challenger: challenger_key,
        bond_lamports,
        is_client_challenge,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct ChallengeTask<'info> {
    #[account(mut)]
    pub task: Account<'info, Task>,
    #[account(
        init,
        payer = challenger,
        space = Challenge::SPACE,
        seeds = [
            b"challenge",
            task.task_id.to_le_bytes().as_ref(),
            challenger.key().as_ref(),
        ],
        bump,
    )]
    pub challenge: Account<'info, Challenge>,
    #[account(mut)]
    pub challenger: Signer<'info>,
    pub system_program: Program<'info, System>,
}
