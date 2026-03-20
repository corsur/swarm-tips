use anchor_lang::prelude::*;
use crate::errors::CoordinationError;
use crate::events::GuessCommitted;
use crate::state::{Game, GameState};
use crate::zk;

pub fn commit_guess(
    ctx: Context<CommitGuess>,
    commitment: [u8; 32],
    proof_a: [u8; 64],
    proof_b: [u8; 128],
    proof_c: [u8; 64],
) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(
        game.state == GameState::Active || game.state == GameState::Committing,
        CoordinationError::InvalidGameState,
    );

    let player_key = ctx.accounts.player.key();
    let is_p1 = player_key == game.player_one;
    let is_p2 = player_key == game.player_two;
    require!(is_p1 || is_p2, CoordinationError::NotAParticipant);

    // Check not already committed
    if is_p1 {
        require!(game.p1_commit == [0u8; 32], CoordinationError::AlreadyCommitted);
    } else {
        require!(game.p2_commit == [0u8; 32], CoordinationError::AlreadyCommitted);
    }

    // Verify the ZK range proof that commitment encodes a valid guess ∈ {0, 1}
    zk::verify_bool_range_proof(&proof_a, &proof_b, &proof_c, &commitment)?;

    let slot = Clock::get()?.slot;
    let game = &mut ctx.accounts.game;

    if is_p1 {
        game.p1_commit = commitment;
        game.p1_commit_slot = slot;
    } else {
        game.p2_commit = commitment;
        game.p2_commit_slot = slot;
    }

    let both_committed =
        game.p1_commit != [0u8; 32] && game.p2_commit != [0u8; 32];

    if game.first_committer == 0 {
        game.first_committer = if is_p1 { 1 } else { 2 };
    }

    game.state = if both_committed {
        GameState::Revealing
    } else {
        GameState::Committing
    };

    // Postcondition: state must have advanced to Committing or Revealing
    require!(
        game.state == GameState::Revealing || game.state == GameState::Committing,
        CoordinationError::InvalidGameState,
    );

    emit!(GuessCommitted {
        game_id: game.game_id,
        player: player_key,
        commit_slot: slot,
    });
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
