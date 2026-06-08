//! `migrate_task` — one-shot realloc of legacy 315-byte `Task` accounts to the
//! current 347-byte layout. C2 added `payout_to: Pubkey` as the LAST field
//! (after `bump`), so the existing 315 bytes are bytewise-identical in the new
//! layout — migration only grows the account and zero-fills the 32-byte tail
//! (`payout_to == Pubkey::default()` == pay the agent, the safe default).
//!
//! Permissionless + idempotent: anyone can pay to migrate any Task; an
//! already-347-byte account returns `Ok(())`. Legacy accounts can't open as
//! `Account<Task>` (size mismatch), so the account is taken as `AccountInfo`
//! and the discriminator / size are checked explicitly.

use anchor_lang::prelude::*;
use anchor_lang::Discriminator;

use crate::errors::ShillbotError;
use crate::state::Task;

const LEGACY_TASK_SIZE: usize = 315;
const CURRENT_TASK_SIZE: usize = Task::SPACE; // 347

pub fn migrate_task(ctx: Context<MigrateTask>) -> Result<()> {
    let task_info = &ctx.accounts.task;

    // Checks: ownership + discriminator (defense-in-depth for raw AccountInfo).
    require_keys_eq!(*task_info.owner, crate::ID, ShillbotError::InvalidTaskState);
    {
        let data = task_info.try_borrow_data()?;
        require!(
            data.len() >= 8 && data[..8] == *Task::DISCRIMINATOR,
            ShillbotError::InvalidTaskState
        );
    }

    let current_size = task_info.data_len();
    if current_size == CURRENT_TASK_SIZE {
        return Ok(()); // idempotent — already migrated
    }
    require!(
        current_size == LEGACY_TASK_SIZE,
        ShillbotError::InvalidTaskState
    );

    // Caller pre-funds the rent diff (a SystemProgram::transfer ix before this
    // one), keeping the handler pure-compute around the resize.
    let new_min = Rent::get()?.minimum_balance(CURRENT_TASK_SIZE);
    require!(
        task_info.lamports() >= new_min,
        ShillbotError::InvalidTaskState
    );

    // Effects: grow. Solana zero-inits the new tail → payout_to = default.
    // Append-only layout means the existing 315 bytes are preserved as-is.
    task_info.resize(CURRENT_TASK_SIZE)?;

    // Postcondition: the account deserializes cleanly at the new size, and the
    // grown 32-byte tail (`payout_to`) is zero-filled by the runtime. Assert the
    // latter explicitly — if a future runtime/toolchain change ever broke the
    // resize zero-fill contract, this fails loudly instead of silently leaving a
    // garbage (non-default) payout route on a migrated task.
    {
        let data = task_info.try_borrow_data()?;
        let t = Task::try_deserialize(&mut &data[..])
            .map_err(|_| error!(ShillbotError::InvalidTaskState))?;
        require!(
            t.payout_to == Pubkey::default(),
            ShillbotError::InvalidPayoutTarget
        );
    }
    msg!(
        "migrated Task from {} to {} bytes",
        LEGACY_TASK_SIZE,
        CURRENT_TASK_SIZE
    );
    Ok(())
}

#[derive(Accounts)]
pub struct MigrateTask<'info> {
    /// CHECK: legacy 315-byte Task accounts can't open as `Account<Task>`; the
    /// handler verifies owner + discriminator + size before resizing.
    #[account(mut)]
    pub task: AccountInfo<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_size_constants() {
        assert_eq!(LEGACY_TASK_SIZE, 315);
        assert_eq!(CURRENT_TASK_SIZE, 347);
    }
}
