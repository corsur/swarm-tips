use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::errors::ShillbotError;
use crate::events::TaskCreated;
use crate::state::{GlobalState, Task, TaskState};

/// Client creates a new task and funds the escrow.
pub fn create_task(
    ctx: Context<CreateTask>,
    escrow_lamports: u64,
    content_hash: [u8; 32],
    deadline: i64,
    submit_margin: i64,
    claim_buffer: i64,
) -> Result<()> {
    let clock = Clock::get()?;

    // Checks
    require!(
        deadline > clock.unix_timestamp,
        ShillbotError::DeadlineExpired
    );
    require!(submit_margin >= 0, ShillbotError::ArithmeticOverflow);
    require!(claim_buffer >= 0, ShillbotError::ArithmeticOverflow);
    // MIN_CLAIM_BUFFER_SECONDS enforced by orchestrator (off-chain) rather than
    // on-chain, so tests and direct callers can use short buffers on devnet.
    require!(escrow_lamports > 0, ShillbotError::ArithmeticOverflow);

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
    task.escrow_lamports = escrow_lamports;
    task.content_hash = content_hash;
    task.video_id_hash = [0u8; 32];
    task.task_nonce = task_nonce;
    task.composite_score = 0;
    task.payment_amount = 0;
    task.deadline = deadline;
    task.submit_margin = submit_margin;
    task.claim_buffer = claim_buffer;
    task.created_at = clock.unix_timestamp;
    task.submitted_at = 0;
    task.verified_at = 0;
    task.challenge_deadline = 0;
    task.client_challenges = 0;
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
    #[account(mut)]
    pub client: Signer<'info>,
    /// CHECK: SlotHashes sysvar for nonce generation.
    #[account(address = anchor_lang::solana_program::sysvar::slot_hashes::id())]
    pub slot_hashes: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}
