#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

//! Extension-credit — the permissionless funding layer for the credit web.
//! Any backer can open an advance to an agent they've extended to: the backer
//! fronts working capital and is recouped, earnings-first, from the agent's
//! routed Shillbot payouts. The `Advance` PDA doubles as the splitter vault —
//! the agent's `payout_to` points at it, and `route_and_recoup` (a permissionless
//! crank) pays the backer back before the agent.
//!
//! Honest limit: an agent can earn under a different identity to dodge the
//! split — mitigated by keeping advances small and fast-recouped, not
//! cryptographically. The treasury (root) participates as just another backer.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;
pub mod transfers;

use instructions::*;

declare_id!("GJLUpJHceGekHBeZMZX4ZYX4xdkK4kFw2tH6uRuQHDqm");

#[program]
pub mod extension_credit {
    use super::*;

    /// Backer fronts `advance_lamports` of working capital to `recipient` and
    /// opens an `Advance` (which is also the earnings-routing vault).
    pub fn open_advance(ctx: Context<OpenAdvance>, advance_lamports: u64) -> Result<()> {
        instructions::open_advance::open_advance(ctx, advance_lamports)
    }

    /// Permissionless crank: split the earnings sitting on the Advance PDA —
    /// backer recouped first (up to the outstanding advance), remainder to the
    /// recipient.
    pub fn route_and_recoup(ctx: Context<RouteAndRecoup>) -> Result<()> {
        instructions::route_and_recoup::route_and_recoup(ctx)
    }

    /// Backer closes a fully-recouped advance, reclaiming the account rent.
    pub fn close_advance(ctx: Context<CloseAdvance>) -> Result<()> {
        instructions::close_advance::close_advance(ctx)
    }

    /// Backer marks a default, sweeping any routed earnings to itself as partial
    /// recoupment and closing the advance (it eats the unrecouped remainder).
    pub fn mark_default(ctx: Context<MarkDefault>) -> Result<()> {
        instructions::mark_default::mark_default(ctx)
    }
}
