use crate::errors::CoordinationError;
use crate::events::GuessCommitted;
use crate::state::{Game, GameState};
use anchor_lang::prelude::*;

pub fn commit_guess(ctx: Context<CommitGuess>, commitment: [u8; 32]) -> Result<()> {
    let player_key = ctx.accounts.player.key();
    // Checks
    let is_p1 = validate_committer(&ctx.accounts.game, &player_key, &commitment)?;
    // Effects
    let slot = Clock::get()?.slot;
    record_commit(&mut ctx.accounts.game, is_p1, commitment, slot)?;
    emit!(GuessCommitted {
        game_id: ctx.accounts.game.game_id,
        player: player_key,
        commit_slot: slot,
    });
    Ok(())
}

/// Pure check phase. Verifies game state, commitment-not-zero, caller is
/// a participant, and the slot they'd write isn't already filled. Returns
/// `true` if the caller is player_one.
pub(crate) fn validate_committer(
    game: &Game,
    player_key: &Pubkey,
    commitment: &[u8; 32],
) -> Result<bool> {
    require!(
        game.state == GameState::Active || game.state == GameState::Committing,
        CoordinationError::InvalidGameState,
    );
    // The all-zeros sentinel is reserved for "not yet committed"; rejecting
    // it means a player who tries to submit it can't commit a second time.
    require!(
        *commitment != [0u8; 32],
        CoordinationError::InvalidGameState
    );

    let is_p1 = *player_key == game.player_one;
    let is_p2 = *player_key == game.player_two;
    require!(is_p1 || is_p2, CoordinationError::NotAParticipant);

    let already = if is_p1 {
        game.p1_commit != [0u8; 32]
    } else {
        game.p2_commit != [0u8; 32]
    };
    require!(!already, CoordinationError::AlreadyCommitted);
    Ok(is_p1)
}

/// Pure effect phase. Stores the commitment, transitions state to
/// Committing or Revealing, sets first_committer the first time through,
/// and asserts the postcondition (caller's slot now equals `commitment`).
pub(crate) fn record_commit(
    game: &mut Game,
    is_p1: bool,
    commitment: [u8; 32],
    slot: u64,
) -> Result<()> {
    if is_p1 {
        game.p1_commit = commitment;
        game.p1_commit_slot = slot;
    } else {
        game.p2_commit = commitment;
        game.p2_commit_slot = slot;
    }
    let both_committed = game.p1_commit != [0u8; 32] && game.p2_commit != [0u8; 32];
    if game.first_committer == 0 {
        game.first_committer = if is_p1 { 1 } else { 2 };
    }
    game.state = if both_committed {
        GameState::Revealing
    } else {
        GameState::Committing
    };
    require!(
        game.state == GameState::Revealing || game.state == GameState::Committing,
        CoordinationError::InvalidGameState,
    );
    let stored = if is_p1 {
        game.p1_commit
    } else {
        game.p2_commit
    };
    require!(stored == commitment, CoordinationError::InvalidGameState);
    Ok(())
}

#[derive(Accounts)]
pub struct CommitGuess<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
    )]
    pub game: Account<'info, Game>,
    pub player: Signer<'info>,
}
