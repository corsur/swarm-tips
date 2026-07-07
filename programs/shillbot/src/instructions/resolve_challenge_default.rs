use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::ChallengeDefaultResolved;
use crate::state::{Challenge, GlobalState, Task, TaskState};
use crate::transfers::transfer_lamports;

/// Permissionless default resolution of a Disputed task once the
/// dispute-resolution window elapses without authority adjudication.
///
/// Closes the liveness hole where `challenge_task` froze escrow + bond
/// forever if the single authority never called `resolve_challenge`.
/// Direction is agent-favoring by design (founder-approved 2026-07-07):
/// the task WAS verified, so the pinned payment/fee execute; the bond
/// returns to the challenger un-slashed because no adjudication happened.
/// A challenge is therefore a bounded delay — never a freeze, never a
/// grief profit.
pub fn resolve_challenge_default(ctx: Context<ResolveChallengeDefault>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let task = &ctx.accounts.task;
    let challenge = &ctx.accounts.challenge;
    let global = &ctx.accounts.global_state;
    // Checks
    validate_default_resolution(task, challenge, global, now)?;
    let bond = challenge.bond_lamports;
    let escrow = task.escrow_lamports;
    let task_id = task.task_id;
    let payment = task.payment_amount;
    let fee = task.fee_amount;
    // Postcondition precheck: conservation over the escrow.
    let total_out = payment
        .checked_add(fee)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(total_out <= escrow, ShillbotError::PaymentExceedsEscrow);
    // Effects
    let challenge = &mut ctx.accounts.challenge;
    challenge.resolved = true;
    challenge.challenger_won = false;
    let task = &mut ctx.accounts.task;
    task.state = TaskState::Resolved;
    // Interactions: pinned payment + fee + remainder, bond back un-slashed.
    distribute_default_resolution(&ctx, payment, fee, escrow, bond)?;
    emit!(ChallengeDefaultResolved {
        task_id,
        payment_amount: payment,
        fee_amount: fee,
        bond_returned: bond,
    });
    Ok(())
}

/// Pure check phase: Disputed state, window enabled, strictly past the
/// resolution deadline, challenge/task binding.
fn validate_default_resolution(
    task: &Task,
    challenge: &Challenge,
    global: &GlobalState,
    now: i64,
) -> Result<()> {
    require!(
        task.state == TaskState::Disputed,
        ShillbotError::InvalidTaskState
    );
    require!(
        challenge.task_id == task.task_id,
        ShillbotError::InvalidTaskState
    );
    let window = global.dispute_resolution_window_seconds;
    require!(window > 0, ShillbotError::DisputeWindowDisabled);
    let resolution_deadline = challenge
        .created_at
        .checked_add(window)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    // Strict inequality — mirrors finalize_task's `now > challenge_deadline`
    // so the authority's window and the permissionless crank never overlap
    // ambiguously at the boundary second.
    require!(
        now > resolution_deadline,
        ShillbotError::DisputeWindowStillOpen
    );
    Ok(())
}

/// Interaction phase: pinned amounts out of the task escrow, bond back to
/// the challenger. Accounts close to client/challenger via constraints.
fn distribute_default_resolution(
    ctx: &Context<ResolveChallengeDefault>,
    payment: u64,
    fee: u64,
    escrow: u64,
    bond: u64,
) -> Result<()> {
    let task_info = ctx.accounts.task.to_account_info();
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
    if bond > 0 {
        let challenge_info = ctx.accounts.challenge.to_account_info();
        transfer_lamports(
            &challenge_info,
            &ctx.accounts.challenger.to_account_info(),
            bond,
        )?;
    }
    Ok(())
}

#[derive(Accounts)]
pub struct ResolveChallengeDefault<'info> {
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
    /// CHECK: Payment recipient — `task.agent`, or `task.payout_to` when the
    /// C2 routing override is set (same rule as finalize/resolve_challenge).
    #[account(
        mut,
        constraint = agent.key() == if task.payout_to == Pubkey::default() { task.agent } else { task.payout_to } @ ShillbotError::NotTaskAgent,
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
    /// CHECK: Treasury for the pinned fee. Validated against GlobalState.treasury.
    #[account(
        mut,
        constraint = treasury.key() == global_state.treasury @ ShillbotError::NotAuthority,
    )]
    pub treasury: AccountInfo<'info>,
}
