use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::ChallengeResolved;
use crate::scoring::compute_payment;
use crate::state::{Challenge, GlobalState, Task, TaskState};

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
        // Return escrow to client
        transfer_lamports(&task_info, &ctx.accounts.client.to_account_info(), escrow)?;
        // Return bond to challenger
        if bond > 0 {
            transfer_lamports(
                &challenge_info,
                &ctx.accounts.challenger.to_account_info(),
                bond,
            )?;
        }
        0
    } else {
        // Agent won: recompute payment to get fee breakdown
        let (payment, fee) = compute_payment(
            task.composite_score,
            global.quality_threshold,
            escrow,
            global.protocol_fee_bps,
        )?;
        if payment > 0 {
            transfer_lamports(&task_info, &ctx.accounts.agent.to_account_info(), payment)?;
        }
        if fee > 0 {
            transfer_lamports(&task_info, &ctx.accounts.treasury.to_account_info(), fee)?;
        }
        // Return remainder escrow to client
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
        if bond > 0 {
            let half_bond = bond
                .checked_div(2)
                .ok_or(ShillbotError::ArithmeticOverflow)?;
            let other_half = bond
                .checked_sub(half_bond)
                .ok_or(ShillbotError::ArithmeticOverflow)?;
            transfer_lamports(
                &challenge_info,
                &ctx.accounts.agent.to_account_info(),
                half_bond,
            )?;
            transfer_lamports(
                &challenge_info,
                &ctx.accounts.treasury.to_account_info(),
                other_half,
            )?;
        }
        bond
    };

    emit!(ChallengeResolved {
        task_id: task.task_id,
        challenger_won,
        bond_slashed,
    });

    Ok(())
}

fn transfer_lamports(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<()> {
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
pub struct ResolveChallenge<'info> {
    #[account(
        mut,
        close = client,
    )]
    pub task: Account<'info, Task>,
    #[account(
        mut,
        close = challenger,
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
