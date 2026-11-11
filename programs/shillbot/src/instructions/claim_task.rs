use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{
    get_stack_height, Instruction, TRANSACTION_LEVEL_STACK_HEIGHT,
};
use anchor_lang::solana_program::sysvar::instructions::{
    self as ix_sysvar, load_current_index_checked, load_instruction_at_checked,
};
use anchor_lang::Discriminator;

use crate::errors::ShillbotError;
use crate::events::TaskClaimed;
use crate::state::{AgentState, GlobalState, Task, TaskState};

/// Agent claims an open task. Enforces minimum time buffer and concurrent claim limit.
///
/// The concurrent claim limit is enforced via the `AgentState` PDA, which tracks
/// how many tasks the agent currently has in Claimed state. This is tamper-proof
/// because the count is maintained on-chain by claim_task, submit_work, and expire_task.
///
/// For `PlatformType::OnChainAction` (`11`), bare speculative claims are rejected (`M1`):
/// `claim_task` must execute at top-level stack height (`1`) with `sysvar::instructions`
/// in `remaining_accounts[0]` and be immediately followed by `submit_work` (or
/// `set_payout_to` then `submit_work`) for the identical `(task, agent)`.
pub fn claim_task(ctx: Context<ClaimTask>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let agent_state = &ctx.accounts.agent_state;
    let global = &ctx.accounts.global_state;
    let agent_key = ctx.accounts.agent.key();

    // Checks: protocol not paused
    require!(!global.paused, ShillbotError::ProtocolPaused);

    // Checks: state
    require!(
        task.state == TaskState::Open,
        ShillbotError::InvalidTaskState
    );

    // The kind-1 self-claim guard (agent != client) was removed: it only
    // blocked the same-wallet wash of a portable "proved theorem X" credential,
    // which a two-wallet poster/worker split bypasses trivially. A credential is
    // only worth what its POSER is worth, so the real defense is off-chain
    // reputation weighting of the poster — the on-chain check added friction and
    // a kind-0/kind-1 inconsistency while protecting nothing a naive consumer
    // wasn't already exposed to. `SelfClaimForbidden` is retained (unused) in the
    // error enum to preserve Anchor error numbering.

    if task.platform == shared::PlatformType::OnChainAction.discriminant() {
        enforce_atomic_submit_bundle(
            ctx.program_id,
            &task.key(),
            &agent_key,
            ctx.remaining_accounts,
        )?;
    }

    // Checks: minimum time buffer before deadline
    let earliest_claim_deadline = clock
        .unix_timestamp
        .checked_add(task.claim_buffer)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        earliest_claim_deadline < task.deadline,
        ShillbotError::ClaimBufferInsufficient
    );

    // Checks: concurrent claim limit via AgentState counter (read from GlobalState)
    require!(
        agent_state.claimed_count < global.max_concurrent_claims,
        ShillbotError::MaxConcurrentClaimsExceeded
    );

    // Effects: update agent state and task
    commit_wallet_claim(
        &mut ctx.accounts.agent_state,
        &mut ctx.accounts.task,
        agent_key,
        ctx.bumps.agent_state,
    )?;

    // Interactions: none
    emit!(TaskClaimed {
        task_id: ctx.accounts.task.task_id,
        agent: agent_key,
    });

    Ok(())
}

fn commit_wallet_claim(
    agent_state: &mut AgentState,
    task: &mut Task,
    agent_key: Pubkey,
    agent_state_bump: u8,
) -> Result<()> {
    // Only set agent and bump on first initialization (freshly zeroed account)
    if agent_state.agent == Pubkey::default() {
        agent_state.agent = agent_key;
        agent_state.bump = agent_state_bump;
    }
    agent_state.claimed_count = agent_state
        .claimed_count
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    // Phase 1 reputation: lifetime claim counter for completion_rate.
    agent_state.total_tasks_claimed = agent_state
        .total_tasks_claimed
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    // Effects: update task
    task.agent = agent_key;
    task.state = TaskState::Claimed;
    Ok(())
}

/// Enforce `M1`: top-level `claim_task` on `OnChainAction` must be followed in
/// the same transaction by `submit_work` (or `set_payout_to` then `submit_work`)
/// targeting the same `task` PDA and `agent` signer.
fn enforce_atomic_submit_bundle(
    program_id: &Pubkey,
    task_key: &Pubkey,
    agent_key: &Pubkey,
    remaining_accounts: &[AccountInfo],
) -> Result<()> {
    require!(
        get_stack_height() == TRANSACTION_LEVEL_STACK_HEIGHT,
        ShillbotError::InvalidInstructionBundle
    );
    let sysvar_info = remaining_accounts
        .first()
        .ok_or(ShillbotError::InvalidInstructionBundle)?;
    require!(
        sysvar_info.key() == ix_sysvar::ID,
        ShillbotError::InvalidInstructionBundle
    );
    let cur = usize::from(
        load_current_index_checked(sysvar_info)
            .map_err(|_| error!(ShillbotError::InvalidInstructionBundle))?,
    );
    let next_idx = cur
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let next_ix = load_instruction_at_checked(next_idx, sysvar_info)
        .map_err(|_| error!(ShillbotError::InvalidInstructionBundle))?;
    let third_ix = cur
        .checked_add(2)
        .and_then(|idx| load_instruction_at_checked(idx, sysvar_info).ok());
    require!(
        is_valid_wallet_claim_bundle(program_id, task_key, agent_key, &next_ix, third_ix.as_ref(),),
        ShillbotError::InvalidInstructionBundle
    );
    Ok(())
}

pub(crate) fn is_valid_wallet_claim_bundle(
    program_id: &Pubkey,
    task_key: &Pubkey,
    agent_key: &Pubkey,
    next_ix: &Instruction,
    third_ix: Option<&Instruction>,
) -> bool {
    if is_matching_submit_work(next_ix, program_id, task_key, agent_key) {
        return true;
    }
    if is_matching_set_payout_to(next_ix, program_id, task_key, agent_key) {
        if let Some(third) = third_ix {
            return is_matching_submit_work(third, program_id, task_key, agent_key);
        }
    }
    false
}

fn is_matching_submit_work(
    ix: &Instruction,
    program_id: &Pubkey,
    task_key: &Pubkey,
    agent_key: &Pubkey,
) -> bool {
    ix.program_id == *program_id
        && ix.data.len() > 12
        && ix
            .data
            .starts_with(crate::instruction::SubmitWork::DISCRIMINATOR)
        && ix.accounts.len() >= 4
        && ix.accounts[0].pubkey == *task_key
        && ix.accounts[0].is_writable
        && ix.accounts[3].pubkey == *agent_key
        && ix.accounts[3].is_signer
}

fn is_matching_set_payout_to(
    ix: &Instruction,
    program_id: &Pubkey,
    task_key: &Pubkey,
    agent_key: &Pubkey,
) -> bool {
    ix.program_id == *program_id
        && ix.data.len() == 40
        && ix
            .data
            .starts_with(crate::instruction::SetPayoutTo::DISCRIMINATOR)
        && ix.accounts.len() >= 2
        && ix.accounts[0].pubkey == *task_key
        && ix.accounts[0].is_writable
        && ix.accounts[1].pubkey == *agent_key
        && ix.accounts[1].is_signer
}

#[derive(Accounts)]
pub struct ClaimTask<'info> {
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
    /// AgentState PDA tracks the agent's concurrent claim count.
    ///
    /// Using `init_if_needed` is acceptable here because:
    /// (a) the agent pays for creation — no cost to the protocol,
    /// (b) AgentState does not hold escrow funds,
    /// (c) the "no init_if_needed for escrow accounts" rule does not apply.
    /// The account is initialized on the agent's first claim and reused thereafter.
    #[account(
        init_if_needed,
        payer = agent,
        space = AgentState::SPACE,
        seeds = [b"agent_state", agent.key().as_ref()],
        bump,
    )]
    pub agent_state: Account<'info, AgentState>,
    #[account(mut)]
    pub agent: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::InstructionData;

    fn submit_ix(program_id: Pubkey, task: Pubkey, agent: Pubkey, content: &[u8]) -> Instruction {
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(task, false),
                AccountMeta::new_readonly(Pubkey::new_unique(), false),
                AccountMeta::new(Pubkey::new_unique(), false),
                AccountMeta::new_readonly(agent, true),
            ],
            data: crate::instruction::SubmitWork {
                content_id: content.to_vec(),
            }
            .data(),
        }
    }

    fn payout_ix(program_id: Pubkey, task: Pubkey, agent: Pubkey) -> Instruction {
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(task, false),
                AccountMeta::new_readonly(agent, true),
            ],
            data: crate::instruction::SetPayoutTo {
                payout_to: Pubkey::new_unique(),
            }
            .data(),
        }
    }

    #[test]
    fn wallet_claim_bundle_accepts_direct_and_payout_routed_submit() {
        let program_id = crate::ID;
        let task = Pubkey::new_unique();
        let agent = Pubkey::new_unique();
        let submit = submit_ix(program_id, task, agent, b"eip155:56:0xabc:0");
        let payout = payout_ix(program_id, task, agent);

        assert!(is_valid_wallet_claim_bundle(
            &program_id,
            &task,
            &agent,
            &submit,
            None
        ));
        assert!(is_valid_wallet_claim_bundle(
            &program_id,
            &task,
            &agent,
            &payout,
            Some(&submit)
        ));
    }

    #[test]
    fn wallet_claim_bundle_rejects_mismatched_task_agent_or_empty_proof() {
        let program_id = crate::ID;
        let task = Pubkey::new_unique();
        let agent = Pubkey::new_unique();
        let wrong_task = submit_ix(program_id, Pubkey::new_unique(), agent, b"proof");
        let wrong_agent = submit_ix(program_id, task, Pubkey::new_unique(), b"proof");
        let empty_proof = submit_ix(program_id, task, agent, b"");
        let payout_only = payout_ix(program_id, task, agent);

        assert!(!is_valid_wallet_claim_bundle(
            &program_id,
            &task,
            &agent,
            &wrong_task,
            None
        ));
        assert!(!is_valid_wallet_claim_bundle(
            &program_id,
            &task,
            &agent,
            &wrong_agent,
            None
        ));
        assert!(!is_valid_wallet_claim_bundle(
            &program_id,
            &task,
            &agent,
            &empty_proof,
            None
        ));
        assert!(!is_valid_wallet_claim_bundle(
            &program_id,
            &task,
            &agent,
            &payout_only,
            None
        ));
    }
}
