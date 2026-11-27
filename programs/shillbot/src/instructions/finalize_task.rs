use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::TaskFinalized;
use crate::state::{AgentState, GlobalState, Task, TaskState};
use crate::transfers::transfer_lamports;

/// Permissionless crank: anyone can call after the challenge deadline passes.
/// Releases payment to agent, fee to treasury, remainder to client.
///
/// Uses the payment and fee amounts stored on the task at verification time,
/// rather than recomputing from current GlobalState parameters. This prevents
/// parameter-change bricking (S-03): if protocol_fee_bps or quality_threshold
/// change between verify and finalize, the stored values are still valid.
pub fn finalize_task(ctx: Context<FinalizeTask>) -> Result<()> {
    let clock = Clock::get()?;
    let task = &ctx.accounts.task;
    // Checks
    require!(
        !ctx.accounts.global_state.paused,
        ShillbotError::ProtocolPaused
    );
    require!(
        task.state == TaskState::Verified,
        ShillbotError::InvalidTaskState
    );
    require!(
        clock.unix_timestamp > task.challenge_deadline,
        ShillbotError::ChallengeWindowOpen
    );
    validate_payout_recipient(&ctx.accounts.agent, task, ctx.remaining_accounts)?;
    let payment_amount = task.payment_amount;
    let fee_amount = task.fee_amount;
    let total_out = payment_amount
        .checked_add(fee_amount)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        total_out <= task.escrow_lamports,
        ShillbotError::PaymentExceedsEscrow
    );
    let remainder = task
        .escrow_lamports
        .checked_sub(total_out)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    // Effects
    let task = &mut ctx.accounts.task;
    task.state = TaskState::Finalized;
    // Update reputation counters only when payment_amount > 0 — preserves
    // pre-#12 behavior. See `state/agent.rs` "Counter semantics".
    if payment_amount > 0 {
        update_agent_stats(
            ctx.remaining_accounts,
            ctx.program_id,
            &task.agent,
            payment_amount,
            task.composite_score,
        )?;
    }
    // Interactions: distribute payment, fee, and remainder.
    distribute_finalized_payment(
        &task.to_account_info(),
        &ctx.accounts.agent.to_account_info(),
        &ctx.accounts.treasury.to_account_info(),
        &ctx.accounts.client.to_account_info(),
        payment_amount,
        fee_amount,
        remainder,
    )?;
    emit!(TaskFinalized {
        task_id: task.task_id,
        agent: task.agent,
        payment_amount,
        fee_amount,
    });
    Ok(())
}

/// Validates that `agent` is the legitimate payout recipient (`task.agent` when
/// `task.payout_to` is unset, `task.payout_to` when active, or `task.agent` as
/// a safe fallback when `task.payout_to` is proven closed/uninitialized via
/// `remaining_accounts`, preventing `InsufficientFundsForRent` or stranded
/// payouts on a closed credit Advance PDA — NEW-HIGH-02).
pub(crate) fn is_valid_payout_destination_info(acc: &AccountInfo, expected_agent: &Pubkey) -> bool {
    if acc.executable || crate::instructions::set_payout_to::is_forbidden_payout_pubkey(&acc.key())
    {
        return false;
    }
    if acc.owner == &anchor_lang::system_program::ID && acc.data_is_empty() {
        return true;
    }
    // Also permit a live `extension-credit` `Advance` vault PDA bound to `expected_agent`.
    if acc.owner == &crate::constants::EXTENSION_CREDIT_PROGRAM_ID
        && acc.data_len() == crate::constants::EXTENSION_CREDIT_ADVANCE_SPACE
    {
        if let Ok(data) = acc.try_borrow_data() {
            let disc = &solana_sha256_hasher::hash(b"account:Advance").to_bytes()[..8];
            if &data[..8] == disc && &data[40..72] == expected_agent.as_ref() {
                return true;
            }
        }
    }
    false
}

pub(crate) fn validate_payout_destination_account(
    acc: &AccountInfo,
    expected_agent: &Pubkey,
) -> Result<()> {
    require!(
        is_valid_payout_destination_info(acc, expected_agent),
        ShillbotError::InvalidPayoutTarget
    );
    Ok(())
}

/// Returns `true` iff a 0-lamport System-owned `payout_to` account may fall
/// back to paying `task.agent` (P2-01):
/// - Always allowed for `PAYOUT_ROUTE_EXTENSION_CREDIT` (`1`) and legacy/unset
///   (`0`) routes (closed Advance PDA recovery).
/// - Blocked for explicit System wallet EOAs (`2`) whenever `payment_amount`
///   covers `zero_byte_rent_floor` (or is 0), preventing an agent from bypassing
///   an immutable EOA payout route by leaving/draining the destination to 0.
/// - Still allowed for `2` when `0 < payment_amount < zero_byte_rent_floor`,
///   which would otherwise fail on Solana with `InsufficientFundsForRent`.
pub(crate) fn can_fallback_on_closed_payout_vault(
    payout_route_kind: u8,
    payment_amount: u64,
    zero_byte_rent_floor: u64,
) -> bool {
    payout_route_kind != crate::state::PAYOUT_ROUTE_SYSTEM_WALLET
        || (payment_amount > 0 && payment_amount < zero_byte_rent_floor)
}

pub(crate) fn validate_payout_recipient(
    agent: &AccountInfo,
    task: &Task,
    remaining_accounts: &[AccountInfo],
) -> Result<()> {
    let zero_byte_rent_floor = Rent::get()?.minimum_balance(0);
    validate_payout_recipient_with_rent_floor(agent, task, remaining_accounts, zero_byte_rent_floor)
}

pub(crate) fn validate_payout_recipient_with_rent_floor(
    agent: &AccountInfo,
    task: &Task,
    remaining_accounts: &[AccountInfo],
    zero_byte_rent_floor: u64,
) -> Result<()> {
    if task.payout_to == Pubkey::default() {
        require!(agent.key() == task.agent, ShillbotError::NotTaskAgent);
        require!(
            !agent.executable
                && !crate::instructions::set_payout_to::is_forbidden_payout_pubkey(&agent.key())
                && agent.owner == &anchor_lang::system_program::ID
                && agent.data_is_empty(),
            ShillbotError::InvalidPayoutTarget
        );
        return Ok(());
    }
    if agent.key() == task.payout_to {
        validate_payout_destination_account(agent, &task.agent)?;
        return Ok(());
    }
    if agent.key() == task.agent {
        let allow_closed_vault_fallback = can_fallback_on_closed_payout_vault(
            task.payout_route_kind,
            task.payment_amount,
            zero_byte_rent_floor,
        );
        let payout_vault_closed_or_invalid = remaining_accounts.iter().any(|acc| {
            acc.key() == task.payout_to
                && ((allow_closed_vault_fallback
                    && acc.lamports() == 0
                    && acc.owner == &anchor_lang::system_program::ID
                    && acc.data_is_empty())
                    || !is_valid_payout_destination_info(acc, &task.agent))
        });
        require!(payout_vault_closed_or_invalid, ShillbotError::NotTaskAgent);
        require!(
            !agent.executable
                && !crate::instructions::set_payout_to::is_forbidden_payout_pubkey(&agent.key())
                && agent.owner == &anchor_lang::system_program::ID
                && agent.data_is_empty(),
            ShillbotError::InvalidPayoutTarget
        );
        return Ok(());
    }
    Err(error!(ShillbotError::NotTaskAgent))
}

/// If an AgentState account is passed in `remaining_accounts`, increment
/// `total_completed`, `total_earned`, and `total_score_sum`. This is optional
/// — callers that don't care about agent stats can omit it.
///
/// Each silent-skip path emits a `msg!` with the reason so the off-chain
/// indexer can detect "the orchestrator forgot to pass AgentState" cases —
/// pre-fix, those failures were invisible in logs.
pub(crate) fn update_agent_stats(
    remaining_accounts: &[AccountInfo],
    program_id: &Pubkey,
    expected_agent: &Pubkey,
    payment_amount: u64,
    composite_score: u64,
) -> Result<()> {
    if remaining_accounts.is_empty() {
        msg!("update_agent_stats: skipped — no remaining_accounts passed (caller did not include AgentState)");
        return Ok(());
    }
    for agent_state_info in remaining_accounts {
        if agent_state_info.owner != program_id {
            continue;
        }
        let mut data = agent_state_info.try_borrow_mut_data()?;
        let mut agent_state = match AgentState::try_deserialize(&mut &data[..]) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if agent_state.agent != *expected_agent {
            continue;
        }

        agent_state.total_completed = agent_state
            .total_completed
            .checked_add(1)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        agent_state.total_earned = agent_state
            .total_earned
            .checked_add(payment_amount)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        agent_state.total_score_sum = agent_state
            .total_score_sum
            .checked_add(composite_score)
            .ok_or(ShillbotError::ArithmeticOverflow)?;

        agent_state.try_serialize(&mut &mut data[..])?;
        return Ok(());
    }
    msg!(
        "update_agent_stats: skipped — no matching AgentState for {} in remaining_accounts",
        expected_agent
    );
    Ok(())
}

/// Transfer payment to agent, fee to treasury, remainder to client.
/// Asserts that total distributed does not exceed the task account's lamport balance.
fn distribute_finalized_payment(
    task_info: &AccountInfo,
    agent_info: &AccountInfo,
    treasury_info: &AccountInfo,
    client_info: &AccountInfo,
    payment: u64,
    fee: u64,
    remainder: u64,
) -> Result<()> {
    // Precondition: total distributed <= task account lamports
    let total_distributed = payment
        .checked_add(fee)
        .ok_or(ShillbotError::ArithmeticOverflow)?
        .checked_add(remainder)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        total_distributed <= task_info.lamports(),
        ShillbotError::PaymentExceedsEscrow
    );

    if payment > 0 {
        transfer_lamports(task_info, agent_info, payment)?;
    }
    if fee > 0 {
        transfer_lamports(task_info, treasury_info, fee)?;
    }
    if remainder > 0 {
        transfer_lamports(task_info, client_info, remainder)?;
    }
    Ok(())
}

#[derive(Accounts)]
pub struct FinalizeTask<'info> {
    #[account(
        mut,
        close = client,
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
    /// CHECK: Payment recipient (C2 — extension-credit routing). Equals
    /// `task.agent` when `task.payout_to` is unset (zero) or when the routed
    /// `task.payout_to` vault is closed (proven via `remaining_accounts`);
    /// otherwise it must be `task.payout_to`. Reputation still accrues to
    /// `task.agent` (see `update_agent_stats`).
    #[account(
        mut,
        constraint = agent.key() == task.agent || (task.payout_to != Pubkey::default() && agent.key() == task.payout_to) @ ShillbotError::NotTaskAgent,
    )]
    pub agent: AccountInfo<'info>,
    /// CHECK: Validated as task.client.
    #[account(
        mut,
        constraint = client.key() == task.client @ ShillbotError::NotTaskClient,
    )]
    pub client: AccountInfo<'info>,
    /// CHECK: Treasury account for protocol fees. Validated against GlobalState.treasury.
    #[account(
        mut,
        constraint = treasury.key() == global_state.treasury @ ShillbotError::NotAuthority,
    )]
    pub treasury: AccountInfo<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        PAYOUT_ROUTE_EXTENSION_CREDIT, PAYOUT_ROUTE_SYSTEM_WALLET, PAYOUT_ROUTE_UNSET_OR_LEGACY,
    };

    fn sample_task_with_route(
        agent: Pubkey,
        payout_to: Pubkey,
        payout_route_kind: u8,
        payment_amount: u64,
    ) -> Task {
        Task {
            task_id: 1,
            client: Pubkey::new_unique(),
            agent,
            state: TaskState::Verified,
            platform: 0,
            escrow_lamports: 10_000_000,
            content_hash: [1u8; 32],
            content_id_hash: [2u8; 32],
            task_nonce: [3u8; 16],
            composite_score: 1_000_000,
            payment_amount,
            fee_amount: 0,
            deadline: 100_000,
            submit_margin: 600,
            claim_buffer: 1_200,
            created_at: 100,
            submitted_at: 200,
            verified_at: 300,
            challenge_deadline: 400,
            attestation_delay_override: 0,
            challenge_window_override: 0,
            verification_timeout_override: 0,
            verification_hash: [4u8; 32],
            requires_approval: 0,
            verification_kind: 0,
            payout_route_kind,
            _reserved: [0u8; 17],
            bump: 255,
            payout_to,
        }
    }

    #[test]
    fn closed_vault_fallback_blocks_explicit_system_wallet_when_payment_covers_rent() {
        let zero_byte_rent: u64 = 890_880;
        let agent_key = Pubkey::new_unique();
        let payout_to_key = Pubkey::new_unique();
        let sys_owner = anchor_lang::system_program::ID;

        let mut agent_lamports = 2_000_000u64;
        let mut agent_data = vec![];
        let agent_info = AccountInfo::new(
            &agent_key,
            false,
            true,
            &mut agent_lamports,
            &mut agent_data,
            &sys_owner,
            false,
            0,
        );

        let mut zero_dest_lamports = 0u64;
        let mut zero_dest_data = vec![];
        let zero_dest_info = AccountInfo::new(
            &payout_to_key,
            false,
            false,
            &mut zero_dest_lamports,
            &mut zero_dest_data,
            &sys_owner,
            false,
            0,
        );

        // Explicit System EOA route with payment >= zero-byte rent: fallback to task.agent MUST be rejected (P2-01).
        let explicit_task = sample_task_with_route(
            agent_key,
            payout_to_key,
            PAYOUT_ROUTE_SYSTEM_WALLET,
            5_000_000,
        );
        assert!(validate_payout_recipient_with_rent_floor(
            &agent_info,
            &explicit_task,
            std::slice::from_ref(&zero_dest_info),
            zero_byte_rent,
        )
        .is_err());

        // Paying the explicit 0-lamport System EOA directly succeeds.
        assert!(validate_payout_recipient_with_rent_floor(
            &zero_dest_info,
            &explicit_task,
            &[],
            zero_byte_rent,
        )
        .is_ok());

        // Explicit System EOA route with sub-rent payment (0 < payment < rent_floor): fallback allowed to avoid bricking.
        let dust_task = sample_task_with_route(
            agent_key,
            payout_to_key,
            PAYOUT_ROUTE_SYSTEM_WALLET,
            zero_byte_rent.saturating_sub(1),
        );
        assert!(validate_payout_recipient_with_rent_floor(
            &agent_info,
            &dust_task,
            std::slice::from_ref(&zero_dest_info),
            zero_byte_rent,
        )
        .is_ok());

        // Extension-credit Advance PDA route closed to 0 lamports: fallback to task.agent allowed.
        let credit_task = sample_task_with_route(
            agent_key,
            payout_to_key,
            PAYOUT_ROUTE_EXTENSION_CREDIT,
            5_000_000,
        );
        assert!(validate_payout_recipient_with_rent_floor(
            &agent_info,
            &credit_task,
            std::slice::from_ref(&zero_dest_info),
            zero_byte_rent,
        )
        .is_ok());

        // Legacy pre-upgrade task (payout_route_kind == 0) closed to 0 lamports: fallback preserved.
        let legacy_task = sample_task_with_route(
            agent_key,
            payout_to_key,
            PAYOUT_ROUTE_UNSET_OR_LEGACY,
            5_000_000,
        );
        assert!(validate_payout_recipient_with_rent_floor(
            &agent_info,
            &legacy_task,
            std::slice::from_ref(&zero_dest_info),
            zero_byte_rent,
        )
        .is_ok());
    }
}
