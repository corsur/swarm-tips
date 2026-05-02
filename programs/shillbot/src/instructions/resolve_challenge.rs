use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::ChallengeResolved;
use crate::state::{AgentState, Challenge, GlobalState, Task, TaskState};
use crate::transfers::transfer_lamports;

/// Squads multisig resolves a dispute.
/// If challenger won: escrow returned to client, bond returned to challenger, agent gets $0.
/// If agent won: payment to agent, bond slashed (portion to agent, portion to treasury),
/// remainder escrow to client.
pub fn resolve_challenge(ctx: Context<ResolveChallenge>, challenger_won: bool) -> Result<()> {
    let task = &ctx.accounts.task;
    let challenge = &ctx.accounts.challenge;
    let global = &ctx.accounts.global_state;

    // Checks: state
    require!(
        task.state == TaskState::Disputed,
        ShillbotError::InvalidTaskState
    );

    // Checks: authority is the multisig
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Checks: challenge belongs to this task
    require!(
        challenge.task_id == task.task_id,
        ShillbotError::InvalidTaskState
    );

    let bond = challenge.bond_lamports;
    let escrow = task.escrow_lamports;

    // Effects: mark resolved
    let challenge = &mut ctx.accounts.challenge;
    challenge.resolved = true;
    challenge.challenger_won = challenger_won;

    let task = &mut ctx.accounts.task;
    task.state = TaskState::Resolved;

    // Interactions: distribute funds
    let task_info = task.to_account_info();
    let challenge_info = challenge.to_account_info();

    let bond_slashed: u64 = if challenger_won {
        // Return escrow to client, return bond to challenger
        transfer_lamports(&task_info, &ctx.accounts.client.to_account_info(), escrow)?;
        if bond > 0 {
            transfer_lamports(
                &challenge_info,
                &ctx.accounts.challenger.to_account_info(),
                bond,
            )?;
        }
        0
    } else {
        // Use stored payment and fee from verification time (S-03 fix).
        let payment = task.payment_amount;
        let fee = task.fee_amount;
        // Distribute escrow: payment to agent, fee to treasury, remainder to client
        if payment > 0 {
            transfer_lamports(&task_info, &ctx.accounts.agent.to_account_info(), payment)?;
        }
        if fee > 0 {
            transfer_lamports(&task_info, &ctx.accounts.treasury.to_account_info(), fee)?;
        }
        let total_out = payment
            .checked_add(fee)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        let remainder = escrow
            .checked_sub(total_out)
            .ok_or(ShillbotError::ArithmeticOverflow)?;
        if remainder > 0 {
            transfer_lamports(
                &task_info,
                &ctx.accounts.client.to_account_info(),
                remainder,
            )?;
        }
        // Slash bond: split between agent and treasury per bond_slash_treasury_bps
        slash_bond(
            &challenge_info,
            &ctx.accounts.agent,
            &ctx.accounts.treasury,
            bond,
            global.bond_slash_treasury_bps,
        )?;
        bond
    };

    // Phase 1 reputation: if challenger won (agent lost) and the caller
    // passed AgentState as a remaining_account, bump total_challenges_lost.
    // Optional pattern matches `finalize_task::update_agent_stats` —
    // omitting the account leaves the counter unchanged but the
    // resolution still completes.
    if challenger_won {
        bump_agent_challenges_lost(ctx.remaining_accounts, ctx.program_id, &task.agent)?;
    }

    emit!(ChallengeResolved {
        task_id: task.task_id,
        challenger_won,
        bond_slashed,
    });

    Ok(())
}

/// Increment `total_challenges_lost` on the agent's AgentState, when passed
/// as the first remaining_account. Mirrors the optional-update pattern in
/// `finalize_task::update_agent_stats`.
fn bump_agent_challenges_lost(
    remaining_accounts: &[AccountInfo],
    program_id: &Pubkey,
    expected_agent: &Pubkey,
) -> Result<()> {
    if remaining_accounts.is_empty() {
        return Ok(());
    }
    let agent_state_info = &remaining_accounts[0];

    if agent_state_info.owner != program_id {
        return Ok(());
    }

    let mut data = agent_state_info.try_borrow_mut_data()?;
    let mut agent_state = match AgentState::try_deserialize(&mut &data[..]) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };

    if agent_state.agent != *expected_agent {
        return Ok(());
    }

    agent_state.total_challenges_lost = agent_state
        .total_challenges_lost
        .checked_add(1)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    agent_state.try_serialize(&mut &mut data[..])?;
    Ok(())
}

/// Slash bond between agent and treasury based on treasury_bps.
/// treasury_bps is in basis points (e.g. 5000 = 50%).
/// Precondition: bond > 0 (caller must verify before calling).
fn slash_bond(
    challenge_info: &AccountInfo,
    agent: &AccountInfo,
    treasury: &AccountInfo,
    bond: u64,
    treasury_bps: u16,
) -> Result<()> {
    // Precondition: bond must be positive — caller should not call with zero bond
    require!(bond > 0, ShillbotError::InsufficientBond);

    // Treasury share = bond * treasury_bps / 10_000 (u128 intermediate)
    let treasury_share_128 = (bond as u128)
        .checked_mul(treasury_bps as u128)
        .ok_or(ShillbotError::ArithmeticOverflow)?
        .checked_div(10_000u128)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let treasury_share =
        u64::try_from(treasury_share_128).map_err(|_| error!(ShillbotError::ArithmeticOverflow))?;
    let agent_share = bond
        .checked_sub(treasury_share)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Postcondition: agent_share + treasury_share == bond (no lamports lost or created)
    let total = agent_share
        .checked_add(treasury_share)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(total == bond, ShillbotError::PaymentExceedsEscrow);

    transfer_lamports(challenge_info, agent, agent_share)?;
    transfer_lamports(challenge_info, treasury, treasury_share)?;
    Ok(())
}

#[derive(Accounts)]
pub struct ResolveChallenge<'info> {
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
        mut,
        close = challenger,
        seeds = [
            b"challenge",
            challenge.task_id.to_le_bytes().as_ref(),
            challenge.challenger.as_ref(),
        ],
        bump = challenge.bump,
    )]
    pub challenge: Account<'info, Challenge>,
    #[account(
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
    /// CHECK: Validated as task.agent.
    #[account(
        mut,
        constraint = agent.key() == task.agent @ ShillbotError::NotTaskAgent,
    )]
    pub agent: AccountInfo<'info>,
    /// CHECK: Validated as task.client.
    #[account(
        mut,
        constraint = client.key() == task.client @ ShillbotError::NotTaskClient,
    )]
    pub client: AccountInfo<'info>,
    /// CHECK: Validated as challenge.challenger.
    #[account(
        mut,
        constraint = challenger.key() == challenge.challenger @ ShillbotError::InvalidTaskState,
    )]
    pub challenger: AccountInfo<'info>,
    /// CHECK: Treasury account for slashed bond portion. Validated against GlobalState.treasury.
    #[account(
        mut,
        constraint = treasury.key() == global_state.treasury @ ShillbotError::NotAuthority,
    )]
    pub treasury: AccountInfo<'info>,
}
