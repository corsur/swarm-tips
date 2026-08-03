//! `migrate_global_config` — one-shot realloc of the singleton `GlobalConfig`
//! PDA from the 107-byte v1 layout to the 115-byte layout that carries
//! `stake_lamports`.
//!
//! Background. The per-game stake used to be the compile-time constant
//! `FIXED_STAKE_LAMPORTS`, so re-pegging Solana required a program upgrade
//! while every EVM chain only needed `setConfig`. That asymmetry had a cost:
//! Solana sat at 0.05 SOL ($3.64) against a $5 EVM anchor because the cheap
//! change got made and the expensive one did not. Moving the stake into
//! `GlobalConfig` makes a re-peg an ordinary authority instruction.
//!
//! Design.
//!   * Takes the account as a raw `AccountInfo` — `Account<GlobalConfig>`
//!     cannot open a 107-byte account against the 115-byte struct, so the
//!     handler would never run.
//!   * Discriminator, PDA seeds and authority are checked explicitly, standing
//!     in for what `Account<...>` normally enforces.
//!   * `stake_lamports` is seeded to `DEFAULT_STAKE_LAMPORTS`, i.e. the value
//!     the program has always enforced. The migration changes NO behaviour;
//!     re-pegging is a separate, deliberate `set_stake_lamports` call.
//!   * Idempotent: an account already at 115 bytes returns Ok, so a retried
//!     transaction is not an error.
//!   * Caller pre-funds the rent difference, matching `migrate_agent_state` —
//!     it keeps the handler pure-compute and the borrow lifetime around
//!     `resize` simple.
//!
//! Layout transition:
//! ```text
//! v1 (107 bytes):                    NEW (115 bytes):
//!   [0..8]    discriminator            [0..8]    discriminator
//!   [8..40]   authority                [8..40]   authority          (preserved)
//!   [40..72]  matchmaker               [40..72]  matchmaker         (preserved)
//!   [72..104] treasury                 [72..104] treasury           (preserved)
//!   [104..106] treasury_split_bps      [104..106] treasury_split_bps(preserved)
//!   [106]     bump                     [106]     bump               (preserved)
//!   --                                 [107..115] stake_lamports = DEFAULT
//! ```
//! Every byte of v1 is a prefix of the new layout, so the migration only grows
//! the account and appends. An account survey on 2026-08-03 found exactly one
//! GlobalConfig on mainnet and one on devnet, both at 107 bytes.

use anchor_lang::prelude::*;
use anchor_lang::Discriminator;

use crate::errors::CoordinationError;
use crate::state::{GlobalConfig, DEFAULT_STAKE_LAMPORTS};

/// One-shot migration of the 107-byte `GlobalConfig` to the 115-byte layout.
/// Idempotent once migrated.
pub fn migrate_global_config(ctx: Context<MigrateGlobalConfig>) -> Result<()> {
    let info = &ctx.accounts.global_config;

    // Checks — these stand in for the typed-account guarantees we gave up by
    // taking a raw AccountInfo.
    require!(
        info.owner == ctx.program_id,
        CoordinationError::InvalidGameState
    );
    let len = info.data_len();
    require!(
        len == GlobalConfig::LEGACY_SPACE || len == GlobalConfig::SPACE,
        CoordinationError::InvalidGameState
    );

    let (authority, disc_ok) = {
        let data = info.try_borrow_data()?;
        let disc_ok = data[..8] == GlobalConfig::DISCRIMINATOR[..];
        let mut key = [0u8; 32];
        key.copy_from_slice(&data[8..40]);
        (Pubkey::from(key), disc_ok)
    };
    require!(disc_ok, CoordinationError::InvalidGameState);
    require!(
        authority == ctx.accounts.authority.key(),
        CoordinationError::NotAuthority
    );

    // Already migrated — retried transactions must not fail.
    if len == GlobalConfig::SPACE {
        msg!("migrate_global_config: already at {} bytes, no-op", len);
        return Ok(());
    }

    // Effects: grow, then append the new field. Rent for the extra 8 bytes is
    // pre-funded by the caller's preceding transfer, exactly as
    // migrate_agent_state does.
    info.resize(GlobalConfig::SPACE)?;
    {
        let mut data = info.try_borrow_mut_data()?;
        data[GlobalConfig::LEGACY_SPACE..GlobalConfig::SPACE]
            .copy_from_slice(&DEFAULT_STAKE_LAMPORTS.to_le_bytes());
    }

    // Postcondition: the account now deserializes as the current struct and
    // carries the stake the program previously hard-coded.
    let migrated = GlobalConfig::try_deserialize(&mut &info.try_borrow_data()?[..])?;
    require!(
        migrated.stake_lamports == DEFAULT_STAKE_LAMPORTS,
        CoordinationError::StakeMismatch
    );
    msg!(
        "migrate_global_config: {} -> {} bytes, stake_lamports={}",
        GlobalConfig::LEGACY_SPACE,
        GlobalConfig::SPACE,
        migrated.stake_lamports
    );
    Ok(())
}

#[derive(Accounts)]
pub struct MigrateGlobalConfig<'info> {
    /// CHECK: opened as a raw AccountInfo because the on-disk 107-byte layout
    /// cannot be deserialized into the 115-byte struct. Owner, discriminator,
    /// PDA seeds and authority are all verified in the handler.
    #[account(mut, seeds = [b"global_config"], bump)]
    pub global_config: AccountInfo<'info>,
    /// Must equal the authority recorded inside the account.
    pub authority: Signer<'info>,
}
