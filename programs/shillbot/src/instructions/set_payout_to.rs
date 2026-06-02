use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::PayoutRouteSet;
use crate::state::{Task, TaskState};

/// The task's agent sets a payment-routing override (C2 — extension-credit).
/// `finalize_task` will then pay `payout_to` (e.g. a credit splitter vault)
/// instead of the agent wallet, while reputation still accrues to `task.agent`.
/// Allowed while the task is in flight, before it finalizes.
pub fn set_payout_to(ctx: Context<SetPayoutTo>, payout_to: Pubkey) -> Result<()> {
    let task = &ctx.accounts.task;

    // Checks: caller is the task's agent; task is still in flight.
    require!(
        ctx.accounts.agent.key() == task.agent,
        ShillbotError::NotTaskAgent
    );
    require!(
        matches!(
            task.state,
            TaskState::Claimed | TaskState::Submitted | TaskState::Approved | TaskState::Verified
        ),
        ShillbotError::InvalidTaskState
    );

    // Effects
    let task_id = task.task_id;
    let agent = task.agent;
    let task = &mut ctx.accounts.task;
    task.payout_to = payout_to;

    emit!(PayoutRouteSet {
        task_id,
        agent,
        payout_to,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SetPayoutTo<'info> {
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
    pub agent: Signer<'info>,
}
