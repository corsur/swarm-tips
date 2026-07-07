use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::errors::ShillbotError;
use crate::events::TaskCreated;
use crate::state::{ClientState, GlobalState, Task, TaskState};

/// Client creates a new task and funds the escrow.
#[allow(clippy::too_many_arguments)]
pub fn create_task(
    ctx: Context<CreateTask>,
    escrow_lamports: u64,
    content_hash: [u8; 32],
    deadline: i64,
    submit_margin: i64,
    claim_buffer: i64,
    platform: u8,
    attestation_delay_override: u32,
    challenge_window_override: u32,
    verification_timeout_override: u32,
    // requires_approval (D1, 2026-05-07): false = oracle path
    // proceeds directly from Submitted; true = mandatory approval
    // gate (matches v4 #11 behavior). Per-task opt-in; campaign-
    // level UX is in the orchestrator.
    requires_approval: bool,
    // verification_kind (2026-07-07): 0 = OracleMetrics (Switchboard
    // verify_task), 1 = DeterministicAttested (attester-signed
    // verify_task_attested). Kind 1 ↔ the LeanProof platform,
    // bidirectionally enforced below. Wire twin:
    // chain-core::verify_schema::VerificationKind (append-only).
    verification_kind: u8,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let client_key = ctx.accounts.client.key();
    let global = &ctx.accounts.global_state;
    // Checks
    validate_create_task_inputs(
        global,
        platform,
        deadline,
        submit_margin,
        claim_buffer,
        escrow_lamports,
        attestation_delay_override,
        challenge_window_override,
        verification_timeout_override,
        verification_kind,
        now,
    )?;
    // Effects: rate limit + global counter + derive nonce + init Task PDA.
    apply_rate_limit(
        &mut ctx.accounts.client_state,
        ctx.bumps.client_state,
        client_key,
        global.rate_limit_window_seconds,
        global.max_tasks_per_rate_window,
        now,
    )?;
    let task_id = ctx.accounts.global_state.task_counter;
    ctx.accounts.global_state.task_counter = ctx
        .accounts
        .global_state
        .task_counter
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let task_nonce = derive_task_nonce(&ctx.accounts.slot_hashes)?;
    init_task_account(
        &mut ctx.accounts.task,
        client_key,
        ctx.bumps.task,
        task_id,
        platform,
        escrow_lamports,
        content_hash,
        task_nonce,
        deadline,
        submit_margin,
        claim_buffer,
        attestation_delay_override,
        challenge_window_override,
        verification_timeout_override,
        requires_approval,
        verification_kind,
        now,
    );
    // Interactions: transfer escrow from client to task PDA.
    transfer_escrow_to_task(&ctx, escrow_lamports)?;
    emit!(TaskCreated {
        task_id,
        client: client_key,
        escrow_lamports,
        deadline,
        task_nonce,
        platform,
    });
    Ok(())
}

/// CPI: transfer `escrow_lamports` from the client wallet into the Task PDA.
fn transfer_escrow_to_task(ctx: &Context<CreateTask>, escrow_lamports: u64) -> Result<()> {
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.client.to_account_info(),
                to: ctx.accounts.task.to_account_info(),
            },
        ),
        escrow_lamports,
    )
}

/// Validate every input that does not depend on Task / ClientState mutation.
/// Pure check phase — all `require!` lives here.
#[allow(clippy::too_many_arguments)]
fn validate_create_task_inputs(
    global: &GlobalState,
    platform: u8,
    deadline: i64,
    submit_margin: i64,
    claim_buffer: i64,
    escrow_lamports: u64,
    attestation_delay_override: u32,
    challenge_window_override: u32,
    verification_timeout_override: u32,
    verification_kind: u8,
    now: i64,
) -> Result<()> {
    require!(!global.paused, ShillbotError::ProtocolPaused);

    require!(
        shared::PlatformType::from_u8(platform).is_some(),
        ShillbotError::InvalidPlatform
    );

    // verification_kind is append-only: 0 | 1 today. Kind 1
    // (DeterministicAttested) ↔ the LeanProof platform, both directions —
    // a deterministic task on an oracle platform (or vice versa) would
    // dead-end at verify time, so reject at the boundary.
    require!(
        verification_kind <= 1,
        ShillbotError::InvalidVerificationKind
    );
    let is_lean = platform == shared::PlatformType::LeanProof.discriminant();
    require!(
        (verification_kind == 1) == is_lean,
        ShillbotError::VerificationKindMismatch
    );

    let platform_bit = 1u16
        .checked_shl(platform as u32)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        global.paused_platforms & platform_bit == 0,
        ShillbotError::PlatformPaused
    );

    require!(deadline > now, ShillbotError::DeadlineExpired);
    require!(submit_margin >= 0, ShillbotError::InvalidParameter);
    require!(claim_buffer >= 0, ShillbotError::InvalidParameter);
    require!(escrow_lamports > 0, ShillbotError::InvalidParameter);

    // Timing-override bounds: 0 means "use the global default", any nonzero
    // value must fall inside the per-field [min, max] window.
    if attestation_delay_override > 0 {
        require!(
            (3_600..=2_592_000).contains(&attestation_delay_override),
            ShillbotError::TimingOverrideOutOfBounds
        );
    }
    if challenge_window_override > 0 {
        require!(
            (3_600..=604_800).contains(&challenge_window_override),
            ShillbotError::TimingOverrideOutOfBounds
        );
    }
    if verification_timeout_override > 0 {
        require!(
            (86_400..=2_592_000).contains(&verification_timeout_override),
            ShillbotError::TimingOverrideOutOfBounds
        );
    }

    Ok(())
}

/// Apply the per-client sliding-window rate limit (Phase 3 blocker #2).
///
/// Pure CEI inside the helper: compute the next-state values, validate,
/// then mutate the ClientState account.
fn apply_rate_limit(
    client_state: &mut Account<'_, ClientState>,
    client_state_bump: u8,
    client_key: Pubkey,
    rate_limit_window_seconds: i64,
    max_tasks_per_rate_window: u32,
    now: i64,
) -> Result<()> {
    let is_first_call = client_state.client == Pubkey::default();
    let elapsed = if is_first_call {
        i64::MAX
    } else {
        now.checked_sub(client_state.window_start_ts)
            .ok_or(ShillbotError::ArithmeticOverflow)?
    };
    let window_expired = elapsed >= rate_limit_window_seconds;
    let new_count: u32 = if is_first_call || window_expired {
        1
    } else {
        client_state
            .tasks_in_window
            .checked_add(1)
            .ok_or(ShillbotError::ArithmeticOverflow)?
    };

    require!(
        new_count <= max_tasks_per_rate_window,
        ShillbotError::RateLimitExceeded
    );

    if is_first_call {
        // FIRST-CALL SENTINEL: detection at `is_first_call` above depends on
        // `client_state.client` being `Pubkey::default()` before this
        // assignment. No future code path may ever reset `client` to default.
        client_state.client = client_key;
        client_state.bump = client_state_bump;
    }
    if is_first_call || window_expired {
        client_state.window_start_ts = now;
    }
    client_state.tasks_in_window = new_count;
    client_state.total_tasks_created = client_state
        .total_tasks_created
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    Ok(())
}

/// Read 16 bytes from the SlotHashes sysvar (offset 8 — past the count
/// prefix) as the task nonce.
///
/// If the sysvar is malformed (truncated below the 24-byte minimum we
/// need), reject the transaction. Pre-fix the handler silently fell back
/// to a zero nonce; that made nonce-collision detection on the client
/// side meaningless.
fn derive_task_nonce(slot_hashes: &AccountInfo) -> Result<[u8; 16]> {
    let data = slot_hashes.data.borrow();
    let start = 8usize;
    let end = start
        .checked_add(16)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(data.len() >= end, ShillbotError::MalformedSlotHashes);
    let mut task_nonce = [0u8; 16];
    task_nonce.copy_from_slice(&data[start..end]);
    Ok(task_nonce)
}

/// Write the Task PDA's initial field set. Pure assignment — no I/O.
#[allow(clippy::too_many_arguments)]
fn init_task_account(
    task: &mut Task,
    client: Pubkey,
    bump: u8,
    task_id: u64,
    platform: u8,
    escrow_lamports: u64,
    content_hash: [u8; 32],
    task_nonce: [u8; 16],
    deadline: i64,
    submit_margin: i64,
    claim_buffer: i64,
    attestation_delay_override: u32,
    challenge_window_override: u32,
    verification_timeout_override: u32,
    requires_approval: bool,
    verification_kind: u8,
    now: i64,
) {
    task.task_id = task_id;
    task.client = client;
    task.agent = Pubkey::default();
    task.state = TaskState::Open;
    task.platform = platform;
    task.escrow_lamports = escrow_lamports;
    task.content_hash = content_hash;
    task.content_id_hash = [0u8; 32];
    task.task_nonce = task_nonce;
    task.composite_score = 0;
    task.payment_amount = 0;
    task.fee_amount = 0;
    task.deadline = deadline;
    task.submit_margin = submit_margin;
    task.claim_buffer = claim_buffer;
    task.created_at = now;
    task.submitted_at = 0;
    task.verified_at = 0;
    task.challenge_deadline = 0;
    task.attestation_delay_override = attestation_delay_override;
    task.challenge_window_override = challenge_window_override;
    task.verification_timeout_override = verification_timeout_override;
    task.verification_hash = [0u8; 32];
    task.requires_approval = u8::from(requires_approval);
    task.verification_kind = verification_kind;
    task._reserved = [0u8; 18];
    task.bump = bump;
}

#[derive(Accounts)]
pub struct CreateTask<'info> {
    #[account(
        mut,
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        init,
        payer = client,
        space = Task::SPACE,
        seeds = [
            b"task",
            global_state.task_counter.to_le_bytes().as_ref(),
            client.key().as_ref(),
        ],
        bump,
    )]
    pub task: Account<'info, Task>,
    /// Per-client rate-limit + lifetime-counter PDA (Phase 3 blocker #2).
    /// `init_if_needed` is allowed because ClientState holds no escrow
    /// funds — only counters and a timestamp — so a PDA-resurrection
    /// attack on a closed account would gain the attacker nothing.
    #[account(
        init_if_needed,
        payer = client,
        space = ClientState::SPACE,
        seeds = [b"client_state", client.key().as_ref()],
        bump,
    )]
    pub client_state: Account<'info, ClientState>,
    #[account(mut)]
    pub client: Signer<'info>,
    /// CHECK: SlotHashes sysvar for nonce generation.
    #[account(address = anchor_lang::solana_program::sysvar::slot_hashes::id())]
    pub slot_hashes: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}
