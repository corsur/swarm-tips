use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskClaimed;
use crate::state::{AgentState, GlobalState, SessionDelegate, Task, TaskState};

/// Session-delegated variant of claim_task. The delegate (MCP session key)
/// signs instead of the agent. The SessionDelegate PDA must have the
/// claim_task permission bit (0x01) set.
pub fn claim_task_session(ctx: Context<ClaimTaskSession>) -> Result<()> {
    let clock = Clock::get()?;
    let agent_key = ctx.accounts.session_delegate.agent;
    // Checks
    validate_claim_session_inputs(
        &ctx.accounts.global_state,
        &ctx.accounts.session_delegate,
        &ctx.accounts.task,
        &ctx.accounts.agent_state,
        clock.unix_timestamp,
    )?;
    // Effects
    commit_claim_state(
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

/// Pure check phase. Protocol-paused, session-bitmask-has-claim,
/// session-not-expired, task-Open, claim-buffer satisfied,
/// concurrent-claim-limit not exceeded.
fn validate_claim_session_inputs(
    global: &GlobalState,
    session: &SessionDelegate,
    task: &Task,
    agent_state: &AgentState,
    now: i64,
) -> Result<()> {
    require!(!global.paused, ShillbotError::ProtocolPaused);
    require!(
        session.allowed_instructions & SessionDelegate::CLAIM_TASK_BIT != 0,
        ShillbotError::InvalidSessionDelegate
    );
    if session.expires_at > 0 {
        require!(now < session.expires_at, ShillbotError::SessionExpired);
    }
    require!(
        task.state == TaskState::Open,
        ShillbotError::InvalidTaskState
    );
    // Deterministic (kind 1) tasks reject self-claims — same arms-length
    // rule as claim_task, keyed on the DELEGATING agent, not the session key.
    if task.verification_kind == 1 {
        require!(
            session.agent != task.client,
            ShillbotError::SelfClaimForbidden
        );
    }
    let earliest_claim_deadline = now
        .checked_add(task.claim_buffer)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        earliest_claim_deadline < task.deadline,
        ShillbotError::ClaimBufferInsufficient
    );
    require!(
        agent_state.claimed_count < global.max_concurrent_claims,
        ShillbotError::MaxConcurrentClaimsExceeded
    );
    Ok(())
}

/// Pure effect phase. Initialize the AgentState slots on first claim,
/// increment claimed_count AND total_tasks_claimed, transition task → Claimed.
/// Both counters must move exactly as they do in `claim_task`: the two paths
/// differ only in who signs, never in what they record.
fn commit_claim_state(
    agent_state: &mut AgentState,
    task: &mut Task,
    agent_key: Pubkey,
    agent_state_bump: u8,
) -> Result<()> {
    if agent_state.agent == Pubkey::default() {
        agent_state.agent = agent_key;
        agent_state.bump = agent_state_bump;
    }
    agent_state.claimed_count = agent_state
        .claimed_count
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    // Phase 1 reputation: lifetime claim counter for completion_rate. This
    // MUST match claim_task — the two paths differ only in who signs (wallet
    // vs delegated session key), not in what they record. Incrementing only in
    // claim_task made an agent's completion_rate depend on which path it used:
    // every session-delegated claim inflated the ratio by counting the later
    // submission without ever counting the claim.
    agent_state.total_tasks_claimed = agent_state
        .total_tasks_claimed
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    task.agent = agent_key;
    task.state = TaskState::Claimed;
    Ok(())
}

#[derive(Accounts)]
pub struct ClaimTaskSession<'info> {
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
    /// Uses `init_if_needed` — see claim_task.rs for justification.
    #[account(
        init_if_needed,
        payer = payer,
        space = AgentState::SPACE,
        seeds = [b"agent_state", session_delegate.agent.as_ref()],
        bump,
    )]
    pub agent_state: Account<'info, AgentState>,
    /// SessionDelegate PDA proves the delegate is authorized by the agent.
    #[account(
        seeds = [
            b"session",
            session_delegate.agent.as_ref(),
            delegate.key().as_ref(),
        ],
        bump = session_delegate.bump,
    )]
    pub session_delegate: Account<'info, SessionDelegate>,
    /// The session key (MCP server) that signs the transaction.
    pub delegate: Signer<'info>,
    /// Pays for AgentState init if needed. Typically the delegate or a relayer.
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_state() -> AgentState {
        AgentState {
            agent: Pubkey::default(),
            claimed_count: 0,
            total_completed: 0,
            total_earned: 0,
            total_score_sum: 0,
            total_tasks_claimed: 0,
            total_challenges_lost: 0,
            _reserved: [0u8; 8],
            bump: 0,
        }
    }

    fn open_task() -> Task {
        Task {
            task_id: 1,
            client: Pubkey::new_unique(),
            agent: Pubkey::default(),
            state: TaskState::Open,
            platform: 5,
            escrow_lamports: 0,
            content_hash: [0u8; 32],
            content_id_hash: [0u8; 32],
            task_nonce: [0u8; 16],
            composite_score: 0,
            payment_amount: 0,
            fee_amount: 0,
            deadline: 0,
            submit_margin: 0,
            claim_buffer: 0,
            created_at: 0,
            submitted_at: 0,
            verified_at: 0,
            challenge_deadline: 0,
            attestation_delay_override: 0,
            challenge_window_override: 0,
            verification_timeout_override: 0,
            verification_hash: [0u8; 32],
            requires_approval: 0,
            verification_kind: 1,
            _reserved: [0u8; 18],
            bump: 0,
            payout_to: Pubkey::default(),
        }
    }

    #[test]
    fn session_claim_bumps_both_counters_like_claim_task() {
        // total_tasks_claimed was previously incremented only in claim_task, so
        // an agent's completion_rate depended on which claim path it used.
        let mut st = agent_state();
        let mut task = open_task();
        let agent = Pubkey::new_unique();

        commit_claim_state(&mut st, &mut task, agent, 1).unwrap();
        assert_eq!(st.claimed_count, 1, "concurrent-claim counter");
        assert_eq!(st.total_tasks_claimed, 1, "lifetime claim counter");

        commit_claim_state(&mut st, &mut task, agent, 1).unwrap();
        assert_eq!(st.claimed_count, 2);
        assert_eq!(st.total_tasks_claimed, 2, "lifetime counter is monotonic");
    }

    #[test]
    fn session_claim_initializes_agent_slot_once() {
        let mut st = agent_state();
        let mut task = open_task();
        let agent = Pubkey::new_unique();

        commit_claim_state(&mut st, &mut task, agent, 7).unwrap();
        assert_eq!(st.agent, agent);
        assert_eq!(st.bump, 7);
        assert_eq!(task.state, TaskState::Claimed);
        assert_eq!(task.agent, agent);
    }
}
