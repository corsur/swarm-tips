use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::ChallengeResolved;
use crate::state::{Challenge, GlobalState, Task, TaskState};
use crate::transfers::transfer_lamports;

/// Squads multisig resolves a dispute.
/// If challenger won: escrow returned to client, bond returned to challenger, agent gets $0.
/// If agent won: payment to agent, bond slashed (portion to agent, portion to treasury),
/// remainder escrow to client.
pub fn resolve_challenge(ctx: Context<ResolveChallenge>, challenger_won: bool) -> Result<()> {
    let task = &ctx.accounts.task;
    let challenge = &ctx.accounts.challenge;
    let global = &ctx.accounts.global_state;

    // Checks: state
    require!(
        task.state == TaskState::Disputed,
        ShillbotError::InvalidTaskState
    );

    // Checks: authority is the multisig
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Checks: challenge belongs to this task
    require!(
        challenge.task_id == task.task_id,
        ShillbotError::InvalidTaskState
    );

    let bond = challenge.bond_lamports;
    let escrow = task.escrow_lamports;

    // Effects: mark resolved
    let challenge = &mut ctx.accounts.challenge;
    challenge.resolved = true;
    challenge.challenger_won = challenger_won;

    let task = &mut ctx.accounts.task;
    task.state = TaskState::Resolved;

    // Interactions: distribute funds
    let task_info = task.to_account_info();
    let challenge_info = challenge.to_account_info();

    let bond_slashed: u64 = if challenger_won {
        // Return escrow to client, return bond to challenger
        transfer_lamports(&task_info, &ctx.accounts.client.to_account_info(), escrow)?;
        if bond > 0 {
            transfer_lamports(
                &challenge_info,
                &ctx.accounts.challenger.to_account_info(),
                bond,
            )?;
        }
        0
    } else {
        // Use stored payment and fee from verification time (S-03 fix).
        let payment = task.payment_amount;
        let fee = task.fee_amount;
        // Distribute escrow: payment to agent, fee to treasury, remainder to client
        if payment > 0 {
            transfer_lamports(&task_info, &ctx.accounts.agent.to_account_info(), payment)?;
        }
        if fee > 0 {
            transfer_lamports(&task_info, &ctx.accounts.treasury.to_account_info(), fee)?;
        }
        let total_out = payment
            .checked_add(fee)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        let remainder = escrow
            .checked_sub(total_out)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        if remainder > 0 {
            transfer_lamports(
                &task_info,
                &ctx.accounts.client.to_account_info(),
                remainder,
            )?;
        }
        // Slash bond: 50% to agent, 50% to treasury
        slash_bond(
            &challenge_info,
            &ctx.accounts.agent,
            &ctx.accounts.treasury,
            bond,
        )?;
        bond
    };

    emit!(ChallengeResolved {
        task_id: task.task_id,
        challenger_won,
        bond_slashed,
    });

    Ok(())
}

/// Slash bond 50/50 between agent and treasury.
/// Precondition: bond > 0 (caller must verify before calling).
fn slash_bond(
    challenge_info: &AccountInfo,
    agent: &AccountInfo,
    treasury: &AccountInfo,
    bond: u64,
) -> Result<()> {
    // Precondition: bond must be positive — caller should not call with zero bond
    require!(bond > 0, ShillbotError::InsufficientBond);

    let half = bond
        .checked_div(2)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let other_half = bond
        .checked_sub(half)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Postcondition: half + other_half == bond (no lamports lost or created)
    let total = half
        .checked_add(other_half)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(total == bond, ShillbotError::PaymentExceedsEscrow);

    transfer_lamports(challenge_info, agent, half)?;
    transfer_lamports(challenge_info, treasury, other_half)?;
    Ok(())
}

#[derive(Accounts)]
pub struct ResolveChallenge<'info> {
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
        mut,
        close = challenger,
        seeds = [
            b"challenge",
            challenge.task_id.to_le_bytes().as_ref(),
            challenge.challenger.as_ref(),
        ],
        bump = challenge.bump,
    )]
    pub challenge: Account<'info, Challenge>,
    #[account(
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
    /// CHECK: Validated as task.agent.
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
    /// CHECK: Validated as challenge.challenger.
    #[account(
        mut,
        constraint = challenger.key() == challenge.challenger @ ShillbotError::InvalidTaskState,
    )]
    pub challenger: AccountInfo<'info>,
    /// CHECK: Treasury account for slashed bond portion. Validated against GlobalState.treasury.
    #[account(
        mut,
        constraint = treasury.key() == global_state.treasury @ ShillbotError::NotAuthority,
    )]
    pub treasury: AccountInfo<'info>,
}
