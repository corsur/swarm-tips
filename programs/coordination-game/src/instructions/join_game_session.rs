use crate::errors::CoordinationError;
use crate::events::GameStarted;
use crate::instructions::session_utils::validate_session_authority;
use crate::instructions::utils::transfer_lamports;
use crate::state::{Game, GameState, PlayerProfile, SessionAuthority, StakeEscrow, Tournament};
use anchor_lang::prelude::*;

/// Session-delegated variant of `join_game`. The session key signs instead
/// of the player wallet.
pub fn join_game_session(ctx: Context<JoinGameSession>) -> Result<()> {
    validate_session_authority(
        &ctx.accounts.session_authority,
        &ctx.accounts.player.key(),
        &ctx.accounts.session_signer.key(),
    )?;

    let game = &ctx.accounts.game;
    require!(
        game.state == GameState::Pending,
        CoordinationError::InvalidGameState
    );
    require!(
        ctx.accounts.player.key() != game.player_one,
        CoordinationError::CannotJoinOwnGame,
    );

    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );

    // Validate the player's escrow has an unconsumed deposit
    let tournament_id = ctx.accounts.tournament.tournament_id;
    require!(
        ctx.accounts
            .escrow
            .validate_for_game(&ctx.accounts.player.key(), tournament_id),
        CoordinationError::EscrowInvalid,
    );

    // Init player profile if needed
    ctx.accounts.player_profile.init_if_new(
        ctx.accounts.player.key(),
        tournament_id,
        ctx.bumps.player_profile,
    );
    require!(
        ctx.accounts.player_profile.tournament_id == tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );

    let stake_lamports = ctx.accounts.game.stake_lamports;
    let player_key = ctx.accounts.player.key();
    let current_slot = Clock::get()?.slot;

    // Effects: commit state before the transfer
    ctx.accounts.game.player_two = player_key;
    ctx.accounts.game.state = GameState::Active;
    ctx.accounts.game.activated_at_slot = current_slot;
    ctx.accounts.escrow.consumed = true;

    // Postcondition: game must now be Active with both players set
    require!(
        ctx.accounts.game.state == GameState::Active,
        CoordinationError::InvalidGameState
    );
    require!(
        ctx.accounts.game.player_two != Pubkey::default(),
        CoordinationError::InvalidGameState
    );

    // Capture values needed for the event before transfer borrows accounts
    let game_id = ctx.accounts.game.game_id;
    let player_one = ctx.accounts.game.player_one;

    // Interactions: transfer player 2 stake from escrow into the game PDA
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
