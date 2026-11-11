use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::{TaskReopened, TaskVerified};
use crate::scoring::compute_payment;
use crate::state::{GlobalState, Task, TaskState};

/// Co-signed oracle verification records `composite_score` and computes payment.
///
/// Legacy Switchboard pull-feed parsing has been retired: `switchboard_feed`
/// MUST be an active signer matching `GlobalState.oracle_authority` or
/// `GlobalState.authority`.
///
/// For `PlatformType::OnChainAction` (`11`), scores must be binary (`0` or
/// `MAX_SCORE`), and a `0` score while the task is still claimable
/// (`now + claim_buffer < deadline`) reopens the task in-place to `TaskState::Open`
/// (emitting `TaskReopened` rather than `TaskVerified`) so a failed proof never
/// burns the client's campaign slot or misleads terminal-event indexers.
pub fn verify_task(
    ctx: Context<VerifyTask>,
    composite_score: u64,
    verification_hash: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    let global = &ctx.accounts.global_state;
    // Checks
    require!(!global.paused, ShillbotError::ProtocolPaused);
    validate_verify_inputs(task, composite_score, verification_hash)?;
    validate_attestation_staleness(task, global, clock.unix_timestamp)?;
    require!(
        is_authorized_oracle_account(global, &ctx.accounts.switchboard_feed)
            && ctx.accounts.switchboard_feed.is_signer,
        ShillbotError::OracleAuthorityMismatch
    );

    if should_reopen_onchain_action(task, composite_score, clock.unix_timestamp) {
        let prev_agent = ctx.accounts.task.agent;
        // Effects: reset per-submission state and reopen task in-place
        reopen_task_in_place(&mut ctx.accounts.task);
        // Interactions: none
        emit!(TaskReopened {
            task_id: ctx.accounts.task.task_id,
            prev_agent,
            verification_hash,
        });
        return Ok(());
    }

    // Effects: payment+fee pinned at verification time so finalize/resolve
    // use the fee that was in effect now, not the current GlobalState fee.
    let (payment_amount, fee_amount) = compute_payment_amounts(task, global, composite_score)?;
    commit_verify_state(
        &mut ctx.accounts.task,
        global,
        composite_score,
        verification_hash,
        payment_amount,
        fee_amount,
        clock.unix_timestamp,
    )?;
    // Interactions: none
    emit!(TaskVerified {
        task_id: ctx.accounts.task.task_id,
        composite_score,
        payment_amount,
        fee_amount,
        verification_hash,
    });
    Ok(())
}

/// Pure check phase — required state, score bounds, non-zero hash.
fn validate_verify_inputs(
    task: &Task,
    composite_score: u64,
    verification_hash: [u8; 32],
) -> Result<()> {
    let required_state = if task.requires_approval == 1 {
        TaskState::Approved
    } else {
        TaskState::Submitted
    };
    require!(
        task.state == required_state,
        ShillbotError::InvalidTaskState
    );
    // Mutual exclusion with `verify_task_attested`: `verify_task`
    // only admits kind 0 (OracleMetrics) tasks.
    require!(
        task.verification_kind == 0,
        ShillbotError::VerificationKindMismatch
    );
    require!(
        composite_score <= shared::MAX_SCORE,
        ShillbotError::ScoreOutOfBounds
    );
    if task.platform == shared::PlatformType::OnChainAction.discriminant() {
        require!(
            composite_score == 0 || composite_score == shared::MAX_SCORE,
            ShillbotError::AttestedScoreNotBinary
        );
    }
    require!(
        verification_hash != [0u8; 32],
        ShillbotError::InvalidVerificationHash
    );
    Ok(())
}

/// Returns `true` when an `OnChainAction` submission scored `0` and still has
/// enough time before `deadline` (`now + claim_buffer < deadline`) for another
/// agent to claim and submit a valid proof.
fn should_reopen_onchain_action(task: &Task, composite_score: u64, now: i64) -> bool {
    task.platform == shared::PlatformType::OnChainAction.discriminant()
        && composite_score == 0
        && now.saturating_add(task.claim_buffer) < task.deadline
}

/// Reset all per-claimant/per-submission fields on `Task` back to `TaskState::Open`
/// while preserving escrow, immutable `content_hash`, deadlines, and overrides.
fn reopen_task_in_place(task: &mut Task) {
    task.state = TaskState::Open;
    task.agent = Pubkey::default();
    task.payout_to = Pubkey::default();
    task.content_id_hash = [0u8; 32];
    task.submitted_at = 0;
    task.verified_at = 0;
    task.challenge_deadline = 0;
    task.composite_score = 0;
    task.payment_amount = 0;
    task.fee_amount = 0;
    task.verification_hash = [0u8; 32];
}

/// Pure effect phase — write verification fields and the Verified
/// transition. Resolves the per-task `challenge_window` override.
/// Shared with `verify_task_attested` so both verify entries pin
/// payment/fee and arm the challenge window identically.
pub(crate) fn commit_verify_state(
    task: &mut Task,
    global: &GlobalState,
    composite_score: u64,
    verification_hash: [u8; 32],
    payment_amount: u64,
    fee_amount: u64,
    now: i64,
) -> Result<()> {
    task.composite_score = composite_score;
    task.payment_amount = payment_amount;
    task.fee_amount = fee_amount;
    task.verified_at = now;
    task.verification_hash = verification_hash;
    let challenge_window = if task.challenge_window_override > 0 {
        i64::from(task.challenge_window_override)
    } else {
        global.challenge_window_seconds
    };
    task.challenge_deadline = now
        .checked_add(challenge_window)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    task.state = TaskState::Verified;
    Ok(())
}

/// True when the oracle slot carries `GlobalState.oracle_authority` or
/// `GlobalState.authority` (first-party co-signed verification replacing the
/// deprecated Switchboard pull feed).
fn is_authorized_oracle_account(global: &GlobalState, account: &AccountInfo) -> bool {
    is_authorized_oracle_pubkey(global, &account.key())
}

fn is_authorized_oracle_pubkey(global: &GlobalState, key: &Pubkey) -> bool {
    *key != Pubkey::default() && (*key == global.oracle_authority || *key == global.authority)
}

/// Compute (payment_amount, fee_amount) for the verification using the
/// current GlobalState protocol_fee_bps and quality_threshold.
fn compute_payment_amounts(
    task: &Task,
    global: &GlobalState,
    composite_score: u64,
) -> Result<(u64, u64)> {
    compute_payment(
        composite_score,
        global.quality_threshold,
        task.escrow_lamports,
        global.protocol_fee_bps,
    )
}

/// Validate that the current timestamp falls within the acceptable staleness window
/// starting at the expected attestation time (`submitted_at + attestation_delay`)
/// through `expected_attestation_time + staleness_window_seconds`.
///
/// Uses per-task override if nonzero, else global default for attestation delay.
fn validate_attestation_staleness(task: &Task, global: &GlobalState, now: i64) -> Result<()> {
    let attestation_delay = if task.attestation_delay_override > 0 {
        i64::from(task.attestation_delay_override)
    } else {
        global.attestation_delay_seconds
    };
    let expected_attestation_time = task
        .submitted_at
        .checked_add(attestation_delay)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let earliest = expected_attestation_time;
    let latest = expected_attestation_time
        .checked_add(global.staleness_window_seconds)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Precondition: window bounds are coherent
    require!(earliest <= latest, ShillbotError::ArithmeticOverflow);

    require!(
        now >= earliest && now <= latest,
        ShillbotError::AttestationStale
    );

    Ok(())
}

#[derive(Accounts)]
pub struct VerifyTask<'info> {
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
    /// Required oracle signer (`GlobalState.oracle_authority` or `GlobalState.authority`).
    /// Field name retained as `switchboard_feed` for Anchor IDL backwards compatibility.
    /// CHECK: Validated as an active signer matching `global_state.oracle_authority`
    /// or `global_state.authority`.
    pub switchboard_feed: AccountInfo<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard for the 2026-05-01 → 2026-05-08 const-lockdown
    /// round-trip: the `SWITCHBOARD_FEED` compile-time const is fully
    /// retired. The 2026-05-01 lockdown shipped a placeholder pubkey
    /// (`11111…112`) without filling in a real value, blocking every
    /// `verify_task` call on every network until the revert. This test
    /// fails-loudly if the const is reintroduced.
    #[test]
    fn switchboard_feed_const_is_not_referenced() {
        let src = include_str!("verify_task.rs");
        // Search only the non-test portion of the file.
        let cutoff = src
            .find("#[cfg(test)]")
            .expect("test module marker should exist");
        let production = &src[..cutoff];
        assert!(
            !production.contains("crate::constants::SWITCHBOARD_FEED"),
            "verify_task.rs must not reference crate::constants::SWITCHBOARD_FEED \
             (const retired 2026-05-08; per-network feed pubkey lives in \
             GlobalState.switchboard_feed). Re-introducing the const is the \
             same trap that shipped the placeholder bug."
        );
        assert!(
            !production.contains("PullFeedAccountData"),
            "verify_task.rs must not reference PullFeedAccountData after Switchboard retirement"
        );
    }

    #[test]
    fn authorized_oracle_pubkey_recognizes_oracle_authority_and_authority() {
        let authority = Pubkey::new_unique();
        let oracle_authority = Pubkey::new_unique();
        let feed = Pubkey::new_unique();
        let stranger = Pubkey::new_unique();
        let global = GlobalState {
            task_counter: 0,
            authority,
            treasury: Pubkey::new_unique(),
            protocol_fee_bps: 100,
            quality_threshold: 200_000,
            challenge_window_seconds: 86_400,
            verification_timeout_seconds: 604_800,
            attestation_delay_seconds: 300,
            staleness_window_seconds: 604_800,
            max_concurrent_claims: 5,
            challenge_bond_multiplier_bps: 2,
            bond_slash_treasury_bps: 5_000,
            oracle_authority,
            paused: false,
            paused_platforms: 0,
            switchboard_feed: feed,
            min_escrow_lamports: 0,
            rate_limit_window_seconds: 0,
            max_tasks_per_rate_window: 0,
            dispute_resolution_window_seconds: 0,
            _reserved: [0; 4],
            bump: 255,
        };
        assert!(is_authorized_oracle_pubkey(&global, &oracle_authority));
        assert!(is_authorized_oracle_pubkey(&global, &authority));
        assert!(!is_authorized_oracle_pubkey(&global, &feed));
        assert!(!is_authorized_oracle_pubkey(&global, &stranger));
        assert!(!is_authorized_oracle_pubkey(&global, &Pubkey::default()));
    }

    #[test]
    fn onchain_action_score_zero_reopens_cleanly_and_resets_payout_route() {
        let client = Pubkey::new_unique();
        let mut task = Task {
            task_id: 77,
            client,
            agent: Pubkey::new_unique(),
            state: TaskState::Submitted,
            platform: shared::PlatformType::OnChainAction.discriminant(),
            escrow_lamports: 5_000_000,
            content_hash: [0xAB; 32],
            content_id_hash: [0xCD; 32],
            task_nonce: [0x11; 16],
            composite_score: 0,
            payment_amount: 0,
            fee_amount: 0,
            deadline: 100_000,
            submit_margin: 3_600,
            claim_buffer: 14_400,
            created_at: 1_000,
            submitted_at: 2_000,
            verified_at: 0,
            challenge_deadline: 0,
            attestation_delay_override: 60,
            challenge_window_override: 900,
            verification_timeout_override: 0,
            verification_hash: [0; 32],
            requires_approval: 0,
            verification_kind: 0,
            _reserved: [0; 18],
            bump: 250,
            payout_to: Pubkey::new_unique(),
        };

        assert!(should_reopen_onchain_action(&task, 0, 50_000));
        assert!(!should_reopen_onchain_action(&task, 0, 90_000));
        assert!(!should_reopen_onchain_action(
            &task,
            shared::MAX_SCORE,
            50_000
        ));
        assert!(validate_verify_inputs(&task, 500_000, [1u8; 32]).is_err());

        reopen_task_in_place(&mut task);
        assert_eq!(task.state, TaskState::Open);
        assert_eq!(task.agent, Pubkey::default());
        assert_eq!(task.payout_to, Pubkey::default());
        assert_eq!(task.content_id_hash, [0u8; 32]);
        assert_eq!(task.submitted_at, 0);
        assert_eq!(task.escrow_lamports, 5_000_000);
        assert_eq!(task.content_hash, [0xAB; 32]);
    }
}
