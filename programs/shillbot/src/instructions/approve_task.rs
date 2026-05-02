use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskApproved;
use crate::state::{Task, TaskState};

/// Phase 3 blocker #3a: client review gate between submission and oracle
/// verification.
///
/// Before this gate existed, an agent submitted work and the oracle
/// verified it directly — a brand client had no way to reject content
/// they considered off-brand or unsafe. With this instruction, the
/// state machine becomes:
///
/// ```text
/// Submitted --(approve_task: client signs)--> Approved
/// Approved --(verify_task: oracle authority)--> Verified
/// ```
///
/// `verify_task` now requires `Approved` (was `Submitted`), so the
/// client's approval is a hard gate before any oracle attestation can
/// score the content and unlock the escrow.
///
/// Reject path is provisioned in the state machine via `expire_task`
/// (T+14d timeout returns escrow to client) and the upcoming
/// `reject_task` instruction (Phase 3 blocker #3a follow-up; tracked
/// separately because it requires reject-reason capture in the on-chain
/// account). For now, a client who wants to reject can simply NOT call
/// `approve_task` and let the timeout-driven expiry return the escrow.
pub fn approve_task(ctx: Context<ApproveTask>) -> Result<()> {
    let task = &ctx.accounts.task;
    let client = &ctx.accounts.client;

    // Checks: state must be Submitted
    require!(
        task.state == TaskState::Submitted,
        ShillbotError::InvalidTaskState
    );

    // Checks: caller must be the task's original client
    require!(task.client == client.key(), ShillbotError::NotTaskClient);

    // Effects: transition Submitted -> Approved
    let task = &mut ctx.accounts.task;
    task.state = TaskState::Approved;

    // Interactions: emit event for off-chain consumers (orchestrator,
    // indexers, frontend dashboards)
    emit!(TaskApproved {
        task_id: task.task_id,
        client: client.key(),
        agent: task.agent,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct ApproveTask<'info> {
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
    pub client: Signer<'info>,
}
