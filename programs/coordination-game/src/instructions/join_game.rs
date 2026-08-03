use crate::errors::CoordinationError;
use crate::events::GameStarted;
use crate::instructions::utils::transfer_lamports;
use crate::state::{Game, GameState, PlayerProfile, StakeEscrow, Tournament};
use anchor_lang::prelude::*;

/// Player 2 joins an existing game. P1 is set at create_game time, so this
/// instruction is P2-only. Transitions the game from Pending to Active.
pub fn join_game(ctx: Context<JoinGame>) -> Result<()> {
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

/// Pure check phase: state, tournament window, not-self-join, escrow valid.
pub(crate) fn validate_join_inputs(
    game: &Game,
    tournament: &Tournament,
    escrow: &StakeEscrow,
    player_key: &Pubkey,
    now: i64,
) -> Result<()> {
    require!(
        game.state == GameState::Pending,
        CoordinationError::InvalidGameState
    );
    require!(
        tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );
    require!(
        *player_key != game.player_one,
        CoordinationError::CannotJoinOwnGame,
    );
    require!(
        escrow.validate_for_game(player_key, tournament.tournament_id, game.stake_lamports),
        CoordinationError::EscrowInvalid,
    );
    Ok(())
}

/// Initialize the PlayerProfile PDA the first time this player participates
/// in this tournament. Returns Ok if the profile is already initialized for
/// this tournament; errors if it's set to a DIFFERENT tournament.
pub(crate) fn init_player_profile_if_new(
    profile: &mut PlayerProfile,
    player_key: Pubkey,
    tournament_id: u64,
    bump: u8,
) -> Result<()> {
    profile.init_if_new(player_key, tournament_id, bump);
    require!(
        profile.tournament_id == tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );
    Ok(())
}

/// Pure effect phase. Sets P2, transitions to Active, consumes escrow.
/// Asserts the postcondition (Active state + both player slots filled).
pub(crate) fn commit_join_state(
    game: &mut Game,
    escrow: &mut StakeEscrow,
    player_key: Pubkey,
    current_slot: u64,
) -> Result<()> {
    game.player_two = player_key;
    game.state = GameState::Active;
    game.activated_at_slot = current_slot;
    escrow.consumed = true;
    require!(
        game.state == GameState::Active,
        CoordinationError::InvalidGameState
    );
    require!(
        game.player_two != Pubkey::default(),
        CoordinationError::InvalidGameState
    );
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
