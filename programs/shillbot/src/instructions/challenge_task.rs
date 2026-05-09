use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::errors::ShillbotError;
use crate::events::TaskChallenged;
use crate::scoring::compute_challenge_bond;
use crate::state::{Challenge, GlobalState, Task, TaskState};

/// Anyone can challenge a verified task by posting a bond during the challenge window.
/// All challengers (including the task's client) pay the standard bond.
pub fn challenge_task(ctx: Context<ChallengeTask>) -> Result<()> {
    let clock = Clock::get()?;
    let challenger_key = ctx.accounts.challenger.key();
    let task = &ctx.accounts.task;
    let global = &ctx.accounts.global_state;
    // Checks
    validate_challenge_inputs(global, task, clock.unix_timestamp)?;
    // Compute bond + classify caller
    let is_client_challenge = challenger_key == task.client;
    let multiplier = u8::try_from(global.challenge_bond_multiplier_bps)
        .map_err(|_| error!(ShillbotError::ArithmeticOverflow))?;
    let bond_lamports = compute_challenge_bond(task.escrow_lamports, multiplier)?;
    // Effects
    init_challenge_state(
        &mut ctx.accounts.challenge,
        &mut ctx.accounts.task,
        challenger_key,
        bond_lamports,
        is_client_challenge,
        clock.unix_timestamp,
        ctx.bumps.challenge,
    );
    // Interactions: transfer bond from challenger to challenge PDA
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
    emit!(TaskChallenged {
        task_id: ctx.accounts.task.task_id,
        challenger: challenger_key,
        bond_lamports,
        is_client_challenge,
    });
    Ok(())
}

/// Pure check phase. Protocol-not-paused, task is Verified, within
/// challenge window.
fn validate_challenge_inputs(global: &GlobalState, task: &Task, now: i64) -> Result<()> {
    require!(!global.paused, ShillbotError::ProtocolPaused);
    require!(
        task.state == TaskState::Verified,
        ShillbotError::InvalidTaskState
    );
    require!(
        now < task.challenge_deadline,
        ShillbotError::ChallengeWindowClosed
    );
    Ok(())
}

/// Pure effect phase. Init the Challenge PDA and transition the task to
/// Disputed.
#[allow(clippy::too_many_arguments)]
fn init_challenge_state(
    challenge: &mut Challenge,
    task: &mut Task,
    challenger_key: Pubkey,
    bond_lamports: u64,
    is_client_challenge: bool,
    now: i64,
    bump: u8,
) {
    challenge.task_id = task.task_id;
    challenge.challenger = challenger_key;
    challenge.bond_lamports = bond_lamports;
    challenge.is_client_challenge = is_client_challenge;
    challenge.created_at = now;
    challenge.resolved = false;
    challenge.challenger_won = false;
    challenge.bump = bump;
    task.state = TaskState::Disputed;
}

#[derive(Accounts)]
pub struct ChallengeTask<'info> {
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
