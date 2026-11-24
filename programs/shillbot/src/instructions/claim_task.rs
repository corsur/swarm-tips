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
/// For `verification_kind == 1` (`LeanProof` / `OnChainAction`) without client
/// pre-approval (`requires_approval == 0`), bare speculative claims AND
/// unverified `[claim_task, submit_work]` locks are rejected (`M2`):
/// `claim_task` must execute at top-level stack height (`1`) with
/// `sysvar::instructions` in `remaining_accounts[0]` and be immediately followed
/// by `[set_payout_to?, submit_work, verify_task_attested(MAX_SCORE)]` co-signed
/// by `global_state.oracle_authority` for the identical `(task, agent)`.
///
/// For `OnChainAction` (`kind == 0`, devnet Switchboard) or approval-gated
/// `LeanProof` (`requires_approval == 1`), `M1` atomic `[claim_task,
/// set_payout_to?, submit_work]` is enforced.
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

    if task.verification_kind == 1 && task.requires_approval == 0 {
        enforce_atomic_verified_bundle(
            ctx.program_id,
            &task.key(),
            &agent_key,
            &global.oracle_authority,
            ctx.remaining_accounts,
        )?;
    } else if task.platform == shared::PlatformType::OnChainAction.discriminant()
        || task.platform == shared::PlatformType::LeanProof.discriminant()
    {
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
    validate_strike_cooldown(agent_state, clock.unix_timestamp)?;

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

pub(crate) fn validate_strike_cooldown(agent_state: &AgentState, now: i64) -> Result<()> {
    if agent_state.last_strike_at > 0 {
        let cooldown_until = agent_state
            .last_strike_at
            .saturating_add(crate::constants::SCORE_ZERO_COOLDOWN_SECONDS);
        require!(now >= cooldown_until, ShillbotError::StrikeCooldownActive);
    }
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

/// Enforce `M2` (Open Competition): top-level `claim_task` on
/// `verification_kind == 1 && requires_approval == 0` (`LeanProof` / `OnChainAction`)
/// MUST be immediately followed in the same transaction by:
/// - `[submit_work, verify_task_attested(MAX_SCORE)]`, OR
/// - `[set_payout_to, submit_work, verify_task_attested(MAX_SCORE)]`
///
/// targeting the identical `(task, agent)` and co-signed by `oracle_authority`.
///
/// This makes speculative claim squatting and unverified submission locks
/// structurally impossible on-chain: a task transitions directly `Open -> Verified`
/// only after the attester has verified a passing proof off-chain.
fn enforce_atomic_verified_bundle(
    program_id: &Pubkey,
    task_key: &Pubkey,
    agent_key: &Pubkey,
    oracle_authority: &Pubkey,
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
    let third_idx = cur
        .checked_add(2)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let next_ix = load_instruction_at_checked(next_idx, sysvar_info)
        .map_err(|_| error!(ShillbotError::InvalidInstructionBundle))?;
    let third_ix = load_instruction_at_checked(third_idx, sysvar_info)
        .map_err(|_| error!(ShillbotError::InvalidInstructionBundle))?;
    let fourth_ix = cur
        .checked_add(3)
        .and_then(|idx| load_instruction_at_checked(idx, sysvar_info).ok());
    require!(
        is_valid_verified_claim_bundle(
            program_id,
            task_key,
            agent_key,
            oracle_authority,
            &next_ix,
            &third_ix,
            fourth_ix.as_ref(),
        ),
        ShillbotError::InvalidInstructionBundle
    );
    Ok(())
}

pub(crate) fn is_valid_verified_claim_bundle(
    program_id: &Pubkey,
    task_key: &Pubkey,
    agent_key: &Pubkey,
    oracle_authority: &Pubkey,
    next_ix: &Instruction,
    third_ix: &Instruction,
    fourth_ix: Option<&Instruction>,
) -> bool {
    if is_matching_submit_work(next_ix, program_id, task_key, agent_key)
        && is_matching_verify_task_attested(third_ix, program_id, task_key, oracle_authority)
    {
        return true;
    }
    if is_matching_set_payout_to(next_ix, program_id, task_key, agent_key)
        && is_matching_submit_work(third_ix, program_id, task_key, agent_key)
    {
        if let Some(fourth) = fourth_ix {
            return is_matching_verify_task_attested(
                fourth,
                program_id,
                task_key,
                oracle_authority,
            );
        }
    }
    false
}

/// Checks that `ix` is a `verify_task_attested` instruction for `task_key`,
/// signed by `oracle_authority`, with `score == shared::MAX_SCORE` and a
/// non-zero `verification_hash`.
pub(crate) fn is_matching_verify_task_attested(
    ix: &Instruction,
    program_id: &Pubkey,
    task_key: &Pubkey,
    oracle_authority: &Pubkey,
) -> bool {
    if ix.program_id != *program_id
        || ix.data.len() != 48
        || !ix
            .data
            .starts_with(crate::instruction::VerifyTaskAttested::DISCRIMINATOR)
        || ix.accounts.len() < 3
        || ix.accounts[0].pubkey != *task_key
        || !ix.accounts[0].is_writable
        || ix.accounts[2].pubkey != *oracle_authority
        || !ix.accounts[2].is_signer
    {
        return false;
    }
    let score_bytes: [u8; 8] = match ix.data[8..16].try_into() {
        Ok(b) => b,
        Err(_) => return false,
    };
    let score = u64::from_le_bytes(score_bytes);
    score == shared::MAX_SCORE && ix.data[16..48].iter().any(|&b| b != 0)
}

/// Enforce `M1`: top-level `claim_task` on `OnChainAction` (`kind == 0`) or
/// approval-gated `LeanProof` must be followed in the same transaction by
/// `submit_work` (or `set_payout_to` then `submit_work`) targeting the same
/// `task` PDA and `agent` signer.
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

    fn verify_attested_ix(
        program_id: Pubkey,
        task: Pubkey,
        attester: Pubkey,
        score: u64,
        verification_hash: [u8; 32],
    ) -> Instruction {
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(task, false),
                AccountMeta::new_readonly(Pubkey::new_unique(), false),
                AccountMeta::new_readonly(attester, true),
            ],
            data: crate::instruction::VerifyTaskAttested {
                score,
                verification_hash,
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

    #[test]
    fn verified_claim_bundle_accepts_direct_and_payout_routed_m2() {
        let program_id = crate::ID;
        let task = Pubkey::new_unique();
        let agent = Pubkey::new_unique();
        let oracle = Pubkey::new_unique();
        let submit = submit_ix(program_id, task, agent, b"lean4:sha256:abcd");
        let payout = payout_ix(program_id, task, agent);
        let verify = verify_attested_ix(program_id, task, oracle, shared::MAX_SCORE, [7u8; 32]);

        assert!(is_valid_verified_claim_bundle(
            &program_id,
            &task,
            &agent,
            &oracle,
            &submit,
            &verify,
            None,
        ));
        assert!(is_valid_verified_claim_bundle(
            &program_id,
            &task,
            &agent,
            &oracle,
            &payout,
            &submit,
            Some(&verify),
        ));
    }

    #[test]
    fn verified_claim_bundle_rejects_score_zero_wrong_attester_or_missing_verify() {
        let program_id = crate::ID;
        let task = Pubkey::new_unique();
        let agent = Pubkey::new_unique();
        let oracle = Pubkey::new_unique();
        let submit = submit_ix(program_id, task, agent, b"lean4:sha256:abcd");
        let payout = payout_ix(program_id, task, agent);
        let verify_ok = verify_attested_ix(program_id, task, oracle, shared::MAX_SCORE, [7u8; 32]);
        let verify_zero = verify_attested_ix(program_id, task, oracle, 0, [7u8; 32]);
        let verify_zero_hash =
            verify_attested_ix(program_id, task, oracle, shared::MAX_SCORE, [0u8; 32]);
        let verify_wrong_oracle = verify_attested_ix(
            program_id,
            task,
            Pubkey::new_unique(),
            shared::MAX_SCORE,
            [7u8; 32],
        );
        let verify_wrong_task = verify_attested_ix(
            program_id,
            Pubkey::new_unique(),
            oracle,
            shared::MAX_SCORE,
            [7u8; 32],
        );

        assert!(!is_valid_verified_claim_bundle(
            &program_id,
            &task,
            &agent,
            &oracle,
            &submit,
            &verify_zero,
            None,
        ));
        assert!(!is_valid_verified_claim_bundle(
            &program_id,
            &task,
            &agent,
            &oracle,
            &submit,
            &verify_zero_hash,
            None,
        ));
        assert!(!is_valid_verified_claim_bundle(
            &program_id,
            &task,
            &agent,
            &oracle,
            &submit,
            &verify_wrong_oracle,
            None,
        ));
        assert!(!is_valid_verified_claim_bundle(
            &program_id,
            &task,
            &agent,
            &oracle,
            &submit,
            &verify_wrong_task,
            None,
        ));
        // [set_payout_to, submit] without trailing verify_task_attested is rejected
        assert!(!is_valid_verified_claim_bundle(
            &program_id,
            &task,
            &agent,
            &oracle,
            &payout,
            &submit,
            None,
        ));
        // Skipping submit_work is rejected
        assert!(!is_valid_verified_claim_bundle(
            &program_id,
            &task,
            &agent,
            &oracle,
            &verify_ok,
            &verify_ok,
            None,
        ));
    }
}
