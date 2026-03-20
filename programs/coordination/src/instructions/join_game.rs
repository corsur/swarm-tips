use anchor_lang::prelude::*;
use anchor_lang::system_program;
use crate::errors::CoordinationError;
use crate::events::GameStarted;
use crate::state::{Game, GameState, PlayerProfile, Tournament};

pub fn join_game(ctx: Context<JoinGame>) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(game.state == GameState::Pending, CoordinationError::InvalidGameState);
    require!(
        ctx.accounts.player.key() != game.player_one,
        CoordinationError::CannotJoinOwnGame,
    );

    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );

    // Init player profile if needed
    let profile = &mut ctx.accounts.player_profile;
    if profile.total_games == 0 && !profile.claimed {
        profile.wallet = ctx.accounts.player.key();
        profile.tournament_id = ctx.accounts.tournament.tournament_id;
        profile.wins = 0;
        profile.total_games = 0;
        profile.score = 0;
        profile.claimed = false;
        profile.bump = ctx.bumps.player_profile;
    }
    require!(
        profile.tournament_id == ctx.accounts.tournament.tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );

    let stake_lamports = ctx.accounts.game.stake_lamports;

    // Transfer player 2 stake into the game PDA
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.player.to_account_info(),
                to: ctx.accounts.game.to_account_info(),
            },
        ),
        stake_lamports,
    )?;

    let game = &mut ctx.accounts.game;
    game.player_two = ctx.accounts.player.key();
    game.state = GameState::Active;

    // Postcondition: game must now be Active with both players set
    require!(game.state == GameState::Active, CoordinationError::InvalidGameState);
    require!(game.player_two != Pubkey::default(), CoordinationError::InvalidGameState);

    emit!(GameStarted {
        game_id: game.game_id,
        tournament_id: game.tournament_id,
        player_one: game.player_one,
        player_two: game.player_two,
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
    pub tournament: Account<'info, Tournament>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}
