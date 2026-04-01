use anchor_lang::prelude::*;

use crate::errors::ShillbotError;
use crate::events::SwitchboardFeedUpdated;
use crate::state::GlobalState;

/// Sets the Switchboard pull feed account used for oracle-attested verification.
/// Only the protocol authority can call this instruction.
pub fn set_switchboard_feed(ctx: Context<SetSwitchboardFeed>, feed: Pubkey) -> Result<()> {
    let global = &ctx.accounts.global_state;

    // Checks: caller is authority
    require!(
        ctx.accounts.authority.key() == global.authority,
        ShillbotError::NotAuthority
    );

    // Checks: feed must not be the zero key
    require!(
        feed != Pubkey::default(),
        ShillbotError::InvalidAttestation
    );

    let old_feed = global.switchboard_feed;

    // Effects
    let global = &mut ctx.accounts.global_state;
    global.switchboard_feed = feed;

    // Interactions
    emit!(SwitchboardFeedUpdated {
        old_feed,
        new_feed: feed,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct SetSwitchboardFeed<'info> {
    #[account(
        mut,
        seeds = [b"shillbot_global"],
        bump = global_state.bump,
    )]
    pub global_state: Account<'info, GlobalState>,
    pub authority: Signer<'info>,
}
