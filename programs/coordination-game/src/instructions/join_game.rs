use crate::errors::CoordinationError;
use crate::events::GameStarted;
use crate::instructions::utils::transfer_lamports;
use crate::state::{Game, GameState, PlayerProfile, StakeEscrow, Tournament};
use anchor_lang::prelude::*;

/// Player 2 joins an existing game. P1 is set at create_game time, so this
/// instruction is P2-only. Transitions the game from Pending to Active.
pub fn join_game(ctx: Context<JoinGame>) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(
        game.state == GameState::Pending,
        CoordinationError::InvalidGameState
    );

    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );

    // Cannot join your own game
    require!(
        ctx.accounts.player.key() != game.player_one,
        CoordinationError::CannotJoinOwnGame,
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

    // Effects: set P2, transition to Active
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

    let game_id = ctx.accounts.game.game_id;
    let player_one = ctx.accounts.game.player_one;

    // Transfer P2 stake from escrow to game PDA
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
pub struct JoinGame<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
    )]
    pub game: Account<'info, Game>,
    #[account(
        init_if_needed,
        payer = player,
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
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}
