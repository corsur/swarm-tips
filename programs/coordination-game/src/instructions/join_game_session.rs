use crate::events::GameStarted;
use crate::instructions::join_game::{
    commit_join_state, init_player_profile_if_new, validate_join_inputs,
};
use crate::instructions::session_utils::validate_session_authority;
use crate::instructions::utils::transfer_lamports;
use crate::state::{Game, PlayerProfile, SessionAuthority, StakeEscrow, Tournament};
use anchor_lang::prelude::*;

/// Session-delegated variant of `join_game`. Session key signs instead of
/// the player wallet. Helpers live in `join_game.rs`; the session variant
/// differs only in the account structure (UncheckedAccount player +
/// session_signer + payer change).
pub fn join_game_session(ctx: Context<JoinGameSession>) -> Result<()> {
    validate_session_authority(
        &ctx.accounts.session_authority,
        &ctx.accounts.player.key(),
        &ctx.accounts.session_signer.key(),
    )?;
    let now = Clock::get()?.unix_timestamp;
    let player_key = ctx.accounts.player.key();
    let tournament_id = ctx.accounts.tournament.tournament_id;
    // Checks
    validate_join_inputs(
        &ctx.accounts.game,
        &ctx.accounts.tournament,
        &ctx.accounts.escrow,
        &player_key,
        now,
    )?;
    init_player_profile_if_new(
        &mut ctx.accounts.player_profile,
        player_key,
        tournament_id,
        ctx.bumps.player_profile,
    )?;
    // Effects
    let stake_lamports = ctx.accounts.game.stake_lamports;
    let current_slot = Clock::get()?.slot;
    commit_join_state(
        &mut ctx.accounts.game,
        &mut ctx.accounts.escrow,
        player_key,
        current_slot,
    )?;
    let game_id = ctx.accounts.game.game_id;
    let player_one = ctx.accounts.game.player_one;
    // Interactions
    transfer_lamports(
        &ctx.accounts.escrow.to_account_info(),
        &ctx.accounts.game.to_account_info(),
        stake_lamports,
    )?;
    emit!(GameStarted {
        game_id,
        tournament_id,
        player_one,
        player_two: player_key,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct JoinGameSession<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
    )]
    pub game: Account<'info, Game>,
    #[account(
        init_if_needed,
        payer = session_signer,
        space = PlayerProfile::SPACE,
        seeds = [
            b"player",
            tournament.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump,
    )]
    pub player_profile: Account<'info, PlayerProfile>,
    #[account(
        mut,
        seeds = [
            b"escrow",
            tournament.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, StakeEscrow>,
    #[account(
        seeds = [b"tournament", game.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    /// CHECK: The player wallet. Not a signer — the session key signs instead.
    /// Verified against session_authority.player in the handler.
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
    #[account(mut)]
    pub session_signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
