use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::constants::{MAX_TASKS_PER_RATE_WINDOW, MIN_ESCROW_LAMPORTS, RATE_LIMIT_WINDOW_SECONDS};
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
) -> Result<()> {
    let clock = Clock::get()?;
    let global = &ctx.accounts.global_state;

    // Checks: protocol not paused
    require!(!global.paused, ShillbotError::ProtocolPaused);

    // Checks: platform is valid (must be a known PlatformType)
    require!(
        shared::PlatformType::from_u8(platform).is_some(),
        ShillbotError::InvalidPlatform
    );

    // Checks: platform not paused (bit N corresponds to PlatformType with value N)
    let platform_bit = 1u16
        .checked_shl(platform as u32)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        global.paused_platforms & platform_bit == 0,
        ShillbotError::PlatformPaused
    );

    require!(
        deadline > clock.unix_timestamp,
        ShillbotError::DeadlineExpired
    );
    require!(submit_margin >= 0, ShillbotError::ArithmeticOverflow);
    require!(claim_buffer >= 0, ShillbotError::ArithmeticOverflow);
    // MIN_CLAIM_BUFFER_SECONDS enforced by orchestrator (off-chain) rather than
    // on-chain, so tests and direct callers can use short buffers on devnet.
    require!(escrow_lamports > 0, ShillbotError::ArithmeticOverflow);

    // Phase 3 blocker #2: enforce MIN_ESCROW_LAMPORTS floor. Sybil
    // operators who control both client and agent can recover most of
    // an escrow round-trip; the minimum floor ensures each round-trip
    // ties up non-trivial capital and accumulates protocol-fee bleed.
    // See `crate::constants::MIN_ESCROW_LAMPORTS` for the cost analysis.
    require!(
        escrow_lamports >= MIN_ESCROW_LAMPORTS,
        ShillbotError::EscrowBelowMinimum
    );

    // Phase 3 blocker #2: per-client task-creation rate limit.
    //
    // Sliding 1-hour window: at most MAX_TASKS_PER_RATE_WINDOW
    // create_task calls per client per RATE_LIMIT_WINDOW_SECONDS.
    // Window resets when the next call lands more than the window
    // duration after the current window's start. Sybil attackers must
    // spawn additional client wallets to exceed the cap; each new
    // wallet pays its own one-time ClientState rent (~$0.13 at typical
    // rent prices) — the dominant per-task cost is the protocol-fee
    // bleed (~$0.50 per task at 1% on the $50 escrow floor), so the
    // primary effect of the rate limit is forcing sybil attackers to
    // maintain more wallets, not the rent cost per se.
    //
    // Pure CEI ordering: compute the next-state values, validate, then
    // mutate. Anchor's transaction atomicity makes the
    // mutate-before-require! shape functionally correct (require!
    // failure reverts everything), but pure CEI keeps the persona's
    // style discipline.
    let client_state = &mut ctx.accounts.client_state;
    let is_first_call = client_state.client == Pubkey::default();
    let elapsed = if is_first_call {
        // First-ever create_task by this client (init_if_needed just
        // zero-initialized the account). No prior window; treat as
        // start of a fresh window.
        i64::MAX
    } else {
        clock
            .unix_timestamp
            .checked_sub(client_state.window_start_ts)
            .ok_or(ShillbotError::ArithmeticOverflow)?
    };
    let window_expired = elapsed >= RATE_LIMIT_WINDOW_SECONDS;
    let new_count: u32 = if is_first_call || window_expired {
        1
    } else {
        client_state
            .tasks_in_window
            .checked_add(1)
            .ok_or(ShillbotError::ArithmeticOverflow)?
    };

    // Checks (pure CEI): both the rate-limit cap AND any future
    // invariants on the counter happen here, before any mutation.
    require!(
        new_count <= MAX_TASKS_PER_RATE_WINDOW,
        ShillbotError::RateLimitExceeded
    );

    // Effects: now that all checks pass, commit the new state.
    if is_first_call {
        // FIRST-CALL SENTINEL: detection at the `is_first_call` check
        // above depends on `client_state.client` being
        // `Pubkey::default()` before this assignment. No future code
        // path may ever reset `client` to default — doing so would
        // silently break the sentinel and re-initialize the rate-limit
        // window on every call from that wallet.
        client_state.client = ctx.accounts.client.key();
        client_state.bump = ctx.bumps.client_state;
    }
    if is_first_call || window_expired {
        client_state.window_start_ts = clock.unix_timestamp;
    }
    client_state.tasks_in_window = new_count;
    client_state.total_tasks_created = client_state
        .total_tasks_created
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Checks: timing override bounds (0 = use global default, nonzero must be in range)
    if attestation_delay_override > 0 {
        // Minimum 1 hour, maximum 30 days
        require!(
            attestation_delay_override >= 3_600,
            ShillbotError::TimingOverrideOutOfBounds
        );
        require!(
            attestation_delay_override <= 2_592_000,
            ShillbotError::TimingOverrideOutOfBounds
        );
    }
    if challenge_window_override > 0 {
        // Minimum 1 hour, maximum 7 days
        require!(
            challenge_window_override >= 3_600,
            ShillbotError::TimingOverrideOutOfBounds
        );
        require!(
            challenge_window_override <= 604_800,
            ShillbotError::TimingOverrideOutOfBounds
        );
    }
    if verification_timeout_override > 0 {
        // Minimum 1 day, maximum 30 days
        require!(
            verification_timeout_override >= 86_400,
            ShillbotError::TimingOverrideOutOfBounds
        );
        require!(
            verification_timeout_override <= 2_592_000,
            ShillbotError::TimingOverrideOutOfBounds
        );
    }

    // Effects: increment counter
    let global = &mut ctx.accounts.global_state;
    let task_id = global.task_counter;
    global.task_counter = global
        .task_counter
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Generate task_nonce from recent slothash (16 bytes)
    let slot_hashes = &ctx.accounts.slot_hashes;
    let data = slot_hashes.data.borrow();
    let mut task_nonce = [0u8; 16];
    // Use the first 16 bytes of slot hashes data (after the count prefix of 8 bytes)
    // as a source of pseudorandomness for the nonce.
    let start = 8usize;
    let end = start
        .checked_add(16)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    if data.len() >= end {
        task_nonce.copy_from_slice(&data[start..end]);
    }

    // Effects: initialize task
    let task = &mut ctx.accounts.task;
    task.task_id = task_id;
    task.client = ctx.accounts.client.key();
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
    task.created_at = clock.unix_timestamp;
    task.submitted_at = 0;
    task.verified_at = 0;
    task.challenge_deadline = 0;
    task.attestation_delay_override = attestation_delay_override;
    task.challenge_window_override = challenge_window_override;
    task.verification_timeout_override = verification_timeout_override;
    task.verification_hash = [0u8; 32];
    task._reserved = [0u8; 20];
    task.bump = ctx.bumps.task;

    // Interactions: transfer escrow from client to task PDA
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.client.to_account_info(),
                to: ctx.accounts.task.to_account_info(),
            },
        ),
        escrow_lamports,
    )?;

    emit!(TaskCreated {
        task_id,
        client: ctx.accounts.client.key(),
        escrow_lamports,
        deadline,
        task_nonce,
        platform,
    });

    Ok(())
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
