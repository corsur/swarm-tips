use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::PayoutRouteSet;
use crate::state::{Task, TaskState};

/// The task's agent sets a payment-routing override (C2 — extension-credit).
/// `finalize_task` (and `resolve_challenge`) will then pay `payout_to` (e.g. a
/// credit splitter vault) instead of the agent wallet, while reputation still
/// accrues to `task.agent`.
///
/// The route is **set once, early, and locked**. This is the binding the
/// extension-credit layer depends on: a backer who fronts an advance and routes
/// the agent's payout to its Advance vault must be able to trust the route stays
/// put through settlement. Two properties enforce that:
///   - **State gate** — allowed only *before the payment is known*
///     (`Claimed | Submitted | Approved`). `Verified` is intentionally excluded:
///     otherwise an agent could observe the computed payout, then flip the route
///     away from the vault before `finalize_task`, defeating recoupment.
///   - **One-shot lock** — once a non-default route is set it cannot be changed.
pub fn set_payout_to(ctx: Context<SetPayoutTo>, payout_to: Pubkey) -> Result<()> {
    let task = &ctx.accounts.task;
    let task_key = task.key();

    // Checks: caller is the task's agent.
    require!(
        ctx.accounts.agent.key() == task.agent,
        ShillbotError::NotTaskAgent
    );
    // State gate: pre-verification only (route locked before payout is known).
    require!(
        matches!(
            task.state,
            TaskState::Claimed | TaskState::Submitted | TaskState::Approved
        ),
        ShillbotError::InvalidTaskState
    );
    // One-shot lock: a route, once set, is immutable.
    require!(
        task.payout_to == Pubkey::default(),
        ShillbotError::PayoutRouteLocked
    );
    // Reject footgun / fund-trap targets: the zero pubkey (a no-op that would
    // also defeat the lock), the task PDA itself (self-pay, swept to the client
    // on close), and the client (hands the agent's payment to the client).
    require!(
        payout_to != Pubkey::default() && payout_to != task_key && payout_to != task.client,
        ShillbotError::InvalidPayoutTarget
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
