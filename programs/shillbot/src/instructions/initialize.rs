use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::state::GlobalState;

/// One-time initialization to create the GlobalState singleton.
pub fn initialize(
    ctx: Context<Initialize>,
    protocol_fee_bps: u16,
    quality_threshold: u64,
    starting_counter: u64,
) -> Result<()> {
    // Checks
    require!(
        protocol_fee_bps >= shared::MIN_PROTOCOL_FEE_BPS,
        ShillbotError::ProtocolFeeBoundsExceeded
    );
    require!(
        protocol_fee_bps <= shared::MAX_PROTOCOL_FEE_BPS,
        ShillbotError::ProtocolFeeBoundsExceeded
    );
    require!(
        quality_threshold <= shared::MAX_SCORE,
        ShillbotError::QualityThresholdBoundsExceeded
    );

    // Effects
    let global = &mut ctx.accounts.global_state;
    global.task_counter = starting_counter;
    global.authority = ctx.accounts.authority.key();
    global.treasury = ctx.accounts.treasury.key();
    global.protocol_fee_bps = protocol_fee_bps;
    global.quality_threshold = quality_threshold;
    global.bump = ctx.bumps.global_state;

    Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = GlobalState::SPACE,
        seeds = [b"shillbot_global"],
        bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: Treasury address stored in GlobalState for fee collection.
    pub treasury: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}
