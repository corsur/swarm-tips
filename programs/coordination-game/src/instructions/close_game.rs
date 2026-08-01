use crate::errors::CoordinationError;
use crate::state::{Game, GameState};
use anchor_lang::prelude::*;

/// Closes a resolved game account and sends the rent-exempt lamports to the caller.
/// Permissionless — any wallet can call to reclaim rent after a game resolves.
pub fn close_game(ctx: Context<CloseGame>) -> Result<()> {
    // Anchor's `close = caller` constraint handles the lamport transfer and
    // discriminator zeroing — nothing to do here beyond the state check.
    require!(
        ctx.accounts.game.state == GameState::Resolved,
        CoordinationError::InvalidGameState,
    );
    Ok(())
}

#[derive(Accounts)]
pub struct CloseGame<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
        close = caller,
    )]
    pub game: Account<'info, Game>,
    #[account(mut)]
    pub caller: Signer<'info>,
}
