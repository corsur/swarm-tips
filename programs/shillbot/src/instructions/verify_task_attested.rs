use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::{TaskReopened, TaskVerified};
use crate::instructions::verify_task::{reopen_task_in_place, should_reopen_onchain_action};
use crate::scoring::compute_payment;
use crate::state::{AgentState, GlobalState, Task, TaskState};

/// Attester-signed verification for `verification_kind = 1`
/// (DeterministicAttested) tasks — the deterministic-check path where an
/// allow-listed attester service (e.g. the pinned-Lean proof checker or
/// OnChainAction attester) re-runs the check and its transaction signature
/// releases payment.
///
/// Trust model vs `verify_task`: the Switchboard path is permissionless and
/// trusts the feed pubkey; THIS path requires the (rotatable)
/// `GlobalState.oracle_authority` as a transaction `Signer` — the first
/// instruction to enforce that field. `verification_hash` is the
/// `chain-core::verify_schema::AttestationCert` digest, binding
/// chain/program/task/statement/policy/artifact/score for the audit trail.
///
/// When `score == 0` on an `OnChainAction` task that is still claimable
/// (`now + claim_buffer < deadline`), the task reopens in-place without
/// changing the 3-account `VerifyTaskAttested` ABI: the caller MUST pass the
/// writable `AgentState` PDA (`[b"agent_state", task.agent.as_ref()]`) as
/// `ctx.remaining_accounts[0]`, decrementing `claimed_count` and recording
/// `last_strike_at = now` for the 60s strike cooldown.
pub fn verify_task_attested(
    ctx: Context<VerifyTaskAttested>,
    score: u64,
    verification_hash: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let global = &ctx.accounts.global_state;
    // Checks
    validate_attested_inputs(
        task,
        global,
        &ctx.accounts.attester.key(),
        score,
        verification_hash,
        clock.unix_timestamp,
    )?;

    if should_reopen_onchain_action(task, score, clock.unix_timestamp) {
        let prev_agent = task.agent;
        apply_reopen_agent_strike(
            ctx.remaining_accounts,
            ctx.program_id,
            &prev_agent,
            clock.unix_timestamp,
        )?;
        reopen_task_in_place(&mut ctx.accounts.task);
        emit!(TaskReopened {
            task_id: ctx.accounts.task.task_id,
            prev_agent,
            verification_hash,
        });
        return Ok(());
    }

    // Effects: payment+fee pinned at verification time (same rule as
    // verify_task / S-03); Verified transition + challenge deadline.
    let (payment_amount, fee_amount) = compute_payment(
        score,
        global.quality_threshold,
        task.escrow_lamports,
        global.protocol_fee_bps,
    )?;
    crate::instructions::verify_task::commit_verify_state(
        &mut ctx.accounts.task,
        global,
        score,
        verification_hash,
        payment_amount,
        fee_amount,
        clock.unix_timestamp,
    )?;
    // Interactions: none
    emit!(TaskVerified {
        task_id: ctx.accounts.task.task_id,
        composite_score: score,
        payment_amount,
        fee_amount,
        verification_hash,
    });
    Ok(())
}

/// Validates and updates the `AgentState` PDA passed via `remaining_accounts[0]`
/// when an `OnChainAction` task reopens on `score == 0`.
fn apply_reopen_agent_strike(
    remaining_accounts: &[AccountInfo],
    program_id: &Pubkey,
    expected_agent: &Pubkey,
    now: i64,
) -> Result<()> {
    let agent_info = remaining_accounts
        .first()
        .ok_or(ShillbotError::MissingAgentState)?;
    require!(agent_info.is_writable, ShillbotError::MissingAgentState);
    require_keys_eq!(
        *agent_info.owner,
        *program_id,
        ShillbotError::InvalidAccountOwner
    );
    let (expected_pda, _) =
        Pubkey::find_program_address(&[b"agent_state", expected_agent.as_ref()], program_id);
    require_keys_eq!(
        agent_info.key(),
        expected_pda,
        ShillbotError::MissingAgentState
    );

    let mut data = agent_info.try_borrow_mut_data()?;
    let mut agent_state = AgentState::try_deserialize(&mut &data[..])
        .map_err(|_| error!(ShillbotError::AgentStateDiscriminatorMismatch))?;
    require_keys_eq!(
        agent_state.agent,
        *expected_agent,
        ShillbotError::AgentStateAgentMismatch
    );
    apply_strike_to_agent_state(&mut agent_state, now);
    agent_state.try_serialize(&mut &mut data[..])?;
    Ok(())
}

pub(crate) fn apply_strike_to_agent_state(agent_state: &mut AgentState, now: i64) {
    agent_state.claimed_count = agent_state.claimed_count.saturating_sub(1);
    agent_state.last_strike_at = now;
}

/// Pure check phase — kind gate, attester allow-list, arms-length,
/// binary score, threshold sanity, required state, non-zero hash, and
/// effective verification timeout upper bound (plus OnChainAction attestation delay).
pub(crate) fn validate_attested_inputs(
    task: &Task,
    global: &GlobalState,
    attester_key: &Pubkey,
    score: u64,
    verification_hash: [u8; 32],
    now: i64,
) -> Result<()> {
    require!(!global.paused, ShillbotError::ProtocolPaused);
    let required_state = if task.requires_approval == 1 {
        TaskState::Approved
    } else {
        TaskState::Submitted
    };
    require!(
        task.state == required_state,
        ShillbotError::InvalidTaskState
    );
    require!(
        task.verification_kind == 1,
        ShillbotError::VerificationKindMismatch
    );
    require!(
        *attester_key == global.oracle_authority,
        ShillbotError::OracleAuthorityMismatch
    );
    // Arms-length: the attester key releasing payment must not be the
    // worker being paid (wash-attestation guard).
    require!(
        global.oracle_authority != task.agent,
        ShillbotError::ArmsLengthViolation
    );
    require!(
        score == 0 || score == shared::MAX_SCORE,
        ShillbotError::AttestedScoreNotBinary
    );
    // Fail loud on the pays-zero parameter edge: threshold == MAX_SCORE
    // makes a passing binary score pay 0 silently (scoring.rs formula).
    require!(
        global.quality_threshold < shared::MAX_SCORE,
        ShillbotError::QualityThresholdTooHighForBinary
    );
    require!(
        verification_hash != [0u8; 32],
        ShillbotError::InvalidVerificationHash
    );

    if task.platform == shared::PlatformType::OnChainAction.discriminant() {
        let eff_delay = if task.attestation_delay_override > 0 {
            i64::from(task.attestation_delay_override)
        } else {
            global.attestation_delay_seconds
        };
        require!(
            now >= task.submitted_at.saturating_add(eff_delay),
            ShillbotError::AttestationStale
        );
    }

    let eff_timeout = if task.verification_timeout_override > 0 {
        i64::from(task.verification_timeout_override)
    } else {
        global.verification_timeout_seconds
    };
    require!(
        now <= task.submitted_at.saturating_add(eff_timeout),
        ShillbotError::VerificationTimeoutExceeded
    );

    Ok(())
}

#[derive(Accounts)]
pub struct VerifyTaskAttested<'info> {
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
    /// The allow-listed attester — must match `GlobalState.oracle_authority`
    /// (validated in the handler with a named error; rotate via
    /// `update_oracle_authority`).
    pub attester: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_global(attester: Pubkey) -> GlobalState {
        GlobalState {
            task_counter: 0,
            authority: Pubkey::new_unique(),
            treasury: Pubkey::new_unique(),
            protocol_fee_bps: 500,
            quality_threshold: 200_000,
            challenge_window_seconds: 86_400,
            verification_timeout_seconds: 3_600,
            attestation_delay_seconds: 60,
            staleness_window_seconds: 300,
            max_concurrent_claims: 5,
            challenge_bond_multiplier_bps: 2,
            bond_slash_treasury_bps: 5_000,
            oracle_authority: attester,
            paused: false,
            paused_platforms: 0,
            switchboard_feed: Pubkey::new_unique(),
            min_escrow_lamports: 0,
            rate_limit_window_seconds: 3_600,
            max_tasks_per_rate_window: 100,
            dispute_resolution_window_seconds: 86_400,
            devnet_enabled: false,
            _reserved: [0; 3],
            bump: 255,
        }
    }

    fn sample_task(platform: u8, submitted_at: i64) -> Task {
        Task {
            task_id: 9,
            client: Pubkey::new_unique(),
            agent: Pubkey::new_unique(),
            state: TaskState::Submitted,
            platform,
            escrow_lamports: 5_000_000,
            content_hash: [0xAB; 32],
            content_id_hash: [0xCD; 32],
            task_nonce: [0x11; 16],
            composite_score: 0,
            payment_amount: 0,
            fee_amount: 0,
            deadline: 100_000,
            submit_margin: 600,
            claim_buffer: 1_200,
            created_at: 100,
            submitted_at,
            verified_at: 0,
            challenge_deadline: 0,
            attestation_delay_override: 60,
            challenge_window_override: 900,
            verification_timeout_override: 0,
            verification_hash: [0; 32],
            requires_approval: 0,
            verification_kind: 1,
            _reserved: [0; 18],
            bump: 250,
            payout_to: Pubkey::default(),
        }
    }

    #[test]
    fn verify_task_attested_enforces_timeout_upper_bound_and_onchain_delay() {
        let attester = Pubkey::new_unique();
        let global = sample_global(attester);
        let task = sample_task(shared::PlatformType::OnChainAction.discriminant(), 1_000);

        // Before attestation delay (1000 + 59 < 1060): rejected
        assert!(validate_attested_inputs(
            &task,
            &global,
            &attester,
            shared::MAX_SCORE,
            [1u8; 32],
            1_059
        )
        .is_err());
        // Inside valid window (1060..=4600): accepted
        assert!(validate_attested_inputs(
            &task,
            &global,
            &attester,
            shared::MAX_SCORE,
            [1u8; 32],
            1_060
        )
        .is_ok());
        assert!(validate_attested_inputs(
            &task,
            &global,
            &attester,
            shared::MAX_SCORE,
            [1u8; 32],
            4_600
        )
        .is_ok());
        // Past verification timeout (1000 + 3600 = 4600): rejected
        assert!(validate_attested_inputs(
            &task,
            &global,
            &attester,
            shared::MAX_SCORE,
            [1u8; 32],
            4_601
        )
        .is_err());
    }

    #[test]
    fn strike_decrements_claimed_count_without_underflow_and_records_timestamp() {
        let mut st = AgentState {
            agent: Pubkey::new_unique(),
            claimed_count: 0,
            total_completed: 0,
            total_earned: 0,
            total_score_sum: 0,
            total_tasks_claimed: 1,
            total_challenges_lost: 0,
            last_strike_at: 0,
            bump: 255,
        };
        apply_strike_to_agent_state(&mut st, 12_345);
        assert_eq!(st.claimed_count, 0);
        assert_eq!(st.last_strike_at, 12_345);

        st.claimed_count = 2;
        apply_strike_to_agent_state(&mut st, 12_400);
        assert_eq!(st.claimed_count, 1);
        assert_eq!(st.last_strike_at, 12_400);
    }
}
