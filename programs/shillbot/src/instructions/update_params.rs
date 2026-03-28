use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::ParamsUpdated;
use crate::state::GlobalState;

/// Authority-only instruction to update tunable protocol parameters.
pub fn update_params(
    ctx: Context<UpdateParams>,
    protocol_fee_bps: u16,
    quality_threshold: u64,
) -> Result<()> {
    let global = &ctx.accounts.global_state;

    // Checks: caller is authority
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Checks: fee within bounds [100, 2500] bps
    require!(
        protocol_fee_bps >= shared::MIN_PROTOCOL_FEE_BPS,
        ShillbotError::ArithmeticOverflow
    );
    require!(
        protocol_fee_bps <= shared::MAX_PROTOCOL_FEE_BPS,
        ShillbotError::ArithmeticOverflow
    );

    // Checks: threshold within bounds
    require!(
        quality_threshold <= shared::MAX_SCORE,
        ShillbotError::ScoreOutOfBounds
    );

    // Effects
    let global = &mut ctx.accounts.global_state;
    global.protocol_fee_bps = protocol_fee_bps;
    global.quality_threshold = quality_threshold;

    emit!(ParamsUpdated {
        protocol_fee_bps,
        quality_threshold,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct UpdateParams<'info> {
    #[account(
        mut,
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
}
