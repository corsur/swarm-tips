use crate::events::GuessCommitted;
use crate::instructions::commit_guess::{record_commit, validate_committer};
use crate::instructions::session_utils::validate_session_authority;
use crate::state::{Game, SessionAuthority};
use anchor_lang::prelude::*;

/// Session-delegated variant of `commit_guess`. The session key signs instead
/// of the player wallet. Helpers live in `commit_guess.rs`; the session
/// variant differs only in the account structure (UncheckedAccount player +
/// session_signer), not the commit protocol.
pub fn commit_guess_session(ctx: Context<CommitGuessSession>, commitment: [u8; 32]) -> Result<()> {
    validate_session_authority(
        &ctx.accounts.session_authority,
        &ctx.accounts.player.key(),
        &ctx.accounts.session_signer.key(),
    )?;
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

#[derive(Accounts)]
pub struct CommitGuessSession<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
    )]
    pub game: Account<'info, Game>,
    /// CHECK: The player wallet. Not a signer — the session key signs instead.
    /// Verified against session_authority.player and game participants in the handler.
    pub player: UncheckedAccount<'info>,
    #[account(
        seeds = [
            b"game_session",
            player.key().as_ref(),
            session_signer.key().as_ref(),
        ],
        bump = session_authority.bump,
    )]
    pub session_authority: Account<'info, SessionAuthority>,
    pub session_signer: Signer<'info>,
}
