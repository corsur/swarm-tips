use anchor_lang::prelude::*;

use crate::errors::ExtensionRegistryError;
use crate::events::GlobalInitialized;
use crate::state::GlobalState;

/// One-time setup: record the authority (default arbiter) and treasury
/// (slashed-bond sink). `init` makes this idempotent-once — the first caller
/// configures the registry. For dogfooding the deployer calls
/// `initialize(root, root)`.
pub fn initialize(ctx: Context<Initialize>, authority: Pubkey, treasury: Pubkey) -> Result<()> {
    // Checks
    require!(
        authority != Pubkey::default(),
        ExtensionRegistryError::InvalidAuthority
    );
    require!(
        treasury != Pubkey::default(),
        ExtensionRegistryError::InvalidTreasury
    );

    // Effects
    let global = &mut ctx.accounts.global_state;
    global.authority = authority;
    global.treasury = treasury;
    global.bump = ctx.bumps.global_state;
    global._reserved = [0u8; 32];

    emit!(GlobalInitialized {
        authority,
        treasury
    });
    Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = GlobalState::SPACE,
        seeds = [b"extension_global"],
        bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
