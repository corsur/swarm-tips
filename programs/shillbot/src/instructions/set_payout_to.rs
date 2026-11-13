use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::PayoutRouteSet;
use crate::state::{Task, TaskState};

/// The task's agent sets a payment-routing override (C2 — extension-credit).
/// `finalize_task` (and `resolve_challenge`) will then pay `payout_to` (e.g. a
/// credit splitter vault) instead of the agent wallet, while reputation still
/// accrues to `task.agent`.
///
/// The route is **set once, early, and locked**. This is the binding the
/// extension-credit layer depends on: a backer who fronts an advance and routes
/// the agent's payout to its Advance vault must be able to trust the route stays
/// put through settlement. Two properties enforce that:
///   - **State gate** — allowed only *before the payment is known*
///     (`Claimed | Submitted | Approved`). `Verified` is intentionally excluded:
///     otherwise an agent could observe the computed payout, then flip the route
///     away from the vault before `finalize_task`, defeating recoupment.
///   - **One-shot lock** — once a non-default route is set it cannot be changed.
pub fn set_payout_to(ctx: Context<SetPayoutTo>, payout_to: Pubkey) -> Result<()> {
    let task = &ctx.accounts.task;
    let task_key = task.key();

    // Checks: caller is the task's agent.
    require!(
        ctx.accounts.agent.key() == task.agent,
        ShillbotError::NotTaskAgent
    );
    // State gate: pre-verification only (route locked before payout is known).
    require!(
        matches!(
            task.state,
            TaskState::Claimed | TaskState::Submitted | TaskState::Approved
        ),
        ShillbotError::InvalidTaskState
    );
    // One-shot lock: a route, once set, is immutable.
    require!(
        task.payout_to == Pubkey::default(),
        ShillbotError::PayoutRouteLocked
    );
    // Reject footgun / fund-trap targets: the zero pubkey (a no-op that would
    // also defeat the lock), the task PDA itself (self-pay, swept to the client
    // on close), the client, programs, and sysvars.
    validate_payout_target_pubkey(&payout_to, &task_key, &task.client)?;

    // When the caller also passes the `payout_to` AccountInfo as `remaining_accounts[0]`,
    // validate its owner and data layout immediately so invalid non-system targets
    // fail at route-set time rather than settlement time.
    if let Some(payout_info) = ctx.remaining_accounts.first() {
        require_keys_eq!(
            payout_info.key(),
            payout_to,
            ShillbotError::InvalidPayoutTarget
        );
        crate::instructions::finalize_task::validate_payout_destination_account(
            payout_info,
            &task.agent,
        )?;
    }

    // Effects
    let task_id = task.task_id;
    let agent = task.agent;
    let task = &mut ctx.accounts.task;
    task.payout_to = payout_to;

    emit!(PayoutRouteSet {
        task_id,
        agent,
        payout_to,
    });
    Ok(())
}

/// Rejects zero, task PDA, client, program IDs, native loaders, and sysvar pubkeys.
pub(crate) fn is_forbidden_payout_pubkey(pk: &Pubkey) -> bool {
    use anchor_lang::solana_program::{bpf_loader_upgradeable, sysvar};
    const ED25519_ID: Pubkey =
        Pubkey::from_str_const("Ed25519SigVerify111111111111111111111111111");
    const SECP256K1_ID: Pubkey =
        Pubkey::from_str_const("KeccakSecp256k11111111111111111111111111111");
    const BPF_LOADER_ID: Pubkey =
        Pubkey::from_str_const("BPFLoader2111111111111111111111111111111111");
    const BPF_LOADER_DEPRECATED_ID: Pubkey =
        Pubkey::from_str_const("BPFLoader1111111111111111111111111111111111");
    const LOADER_V4_ID: Pubkey =
        Pubkey::from_str_const("LoaderV411111111111111111111111111111111111");

    *pk == Pubkey::default()
        || *pk == crate::ID
        || *pk == crate::constants::EXTENSION_CREDIT_PROGRAM_ID
        || *pk == anchor_lang::system_program::ID
        || *pk == sysvar::clock::ID
        || *pk == sysvar::rent::ID
        || *pk == sysvar::slot_hashes::ID
        || *pk == sysvar::instructions::ID
        || *pk == sysvar::epoch_schedule::ID
        || *pk == sysvar::stake_history::ID
        || *pk == sysvar::recent_blockhashes::ID
        || *pk == sysvar::fees::ID
        || *pk == sysvar::rewards::ID
        || *pk == sysvar::slot_history::ID
        || *pk == ED25519_ID
        || *pk == SECP256K1_ID
        || *pk == BPF_LOADER_ID
        || *pk == BPF_LOADER_DEPRECATED_ID
        || *pk == LOADER_V4_ID
        || *pk == bpf_loader_upgradeable::ID
}

pub(crate) fn validate_payout_target_pubkey(
    payout_to: &Pubkey,
    task_key: &Pubkey,
    client_key: &Pubkey,
) -> Result<()> {
    require!(
        *payout_to != *task_key
            && *payout_to != *client_key
            && !is_forbidden_payout_pubkey(payout_to),
        ShillbotError::InvalidPayoutTarget
    );
    Ok(())
}

#[derive(Accounts)]
pub struct SetPayoutTo<'info> {
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
    pub agent: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::sysvar;

    #[test]
    fn payout_target_rejects_sysvars_programs_task_and_client() {
        let task_key = Pubkey::new_unique();
        let client_key = Pubkey::new_unique();
        let valid_wallet = Pubkey::new_unique();

        assert!(validate_payout_target_pubkey(&valid_wallet, &task_key, &client_key).is_ok());
        assert!(validate_payout_target_pubkey(&Pubkey::default(), &task_key, &client_key).is_err());
        assert!(validate_payout_target_pubkey(&task_key, &task_key, &client_key).is_err());
        assert!(validate_payout_target_pubkey(&client_key, &task_key, &client_key).is_err());
        assert!(validate_payout_target_pubkey(&crate::ID, &task_key, &client_key).is_err());
        assert!(validate_payout_target_pubkey(
            &anchor_lang::system_program::ID,
            &task_key,
            &client_key
        )
        .is_err());
        assert!(validate_payout_target_pubkey(&sysvar::clock::ID, &task_key, &client_key).is_err());
        assert!(
            validate_payout_target_pubkey(&sysvar::instructions::ID, &task_key, &client_key)
                .is_err()
        );
        assert!(validate_payout_target_pubkey(&sysvar::rent::ID, &task_key, &client_key).is_err());
    }
}
