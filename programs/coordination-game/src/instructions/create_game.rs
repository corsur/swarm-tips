use crate::errors::CoordinationError;
use crate::events::GameCreated;
use crate::instructions::utils::{init_game, transfer_lamports, validate_tournament_cutoff};
use crate::state::{
    Game, GameCounter, GameState, GlobalConfig, PlayerProfile, StakeEscrow, Tournament,
    FIXED_STAKE_LAMPORTS, MATCHUP_TYPE_UNSET,
};
use anchor_lang::prelude::*;

/// Player creates a game with a matchmaker-attested matchup commitment.
///
/// The matchmaker co-signs to prove the commitment is legitimate (prevents players
/// from forging their own commitment and knowing the matchup type). The player
/// pays all gas. The matchmaker never pays.
///
/// The matchup_commitment is SHA-256(R_matchup) where R_matchup[31] & 1 encodes
/// the matchup type (0 = same team, 1 = different teams). The actual matchup_type
/// is revealed during the first guess reveal, after both players have committed
/// their guesses (so neither can change their guess based on matchup knowledge).
pub fn create_game(
    ctx: Context<CreateGame>,
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let player_key = ctx.accounts.player.key();
    let tournament_id = ctx.accounts.tournament.tournament_id;
    // Checks
    validate_create_inputs(
        stake_lamports,
        matchup_commitment,
        ctx.accounts.matchmaker.key(),
        ctx.accounts.global_config.matchmaker,
        &ctx.accounts.tournament,
        &ctx.accounts.escrow,
        &player_key,
        tournament_id,
        now,
    )?;
    // Effects
    let game_id = commit_creation_state(
        &mut ctx.accounts.game_counter,
        &mut ctx.accounts.game,
        &mut ctx.accounts.player_profile,
        &mut ctx.accounts.escrow,
        tournament_id,
        player_key,
        stake_lamports,
        matchup_commitment,
        now,
        ctx.bumps.game,
        ctx.bumps.player_profile,
    )?;
    // Interactions: move the stake from escrow to the Game PDA.
    transfer_lamports(
        &ctx.accounts.escrow.to_account_info(),
        &ctx.accounts.game.to_account_info(),
        stake_lamports,
    )?;
    emit!(GameCreated {
        game_id,
        tournament_id,
        player_one: player_key,
        stake_lamports,
    });
    Ok(())
}

/// Pure check phase shared between `create_game` and `create_game_session`
/// (the session variant has its own `validate_session_authority` step in
/// front; the rule set after that is identical).
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_create_inputs(
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
    matchmaker_key: Pubkey,
    expected_matchmaker: Pubkey,
    tournament: &Tournament,
    escrow: &StakeEscrow,
    player_key: &Pubkey,
    tournament_id: u64,
    now: i64,
) -> Result<()> {
    require!(
        stake_lamports == FIXED_STAKE_LAMPORTS,
        CoordinationError::StakeMismatch
    );
    require!(
        matchmaker_key == expected_matchmaker,
        CoordinationError::NotMatchmaker
    );
    require!(
        matchup_commitment != [0u8; 32],
        CoordinationError::InvalidGameState
    );
    require!(
        tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );
    validate_tournament_cutoff(now, tournament.end_time)?;
    require!(
        escrow.validate_for_game(player_key, tournament_id),
        CoordinationError::EscrowInvalid,
    );
    Ok(())
}

/// Pure effect phase shared between `create_game` and `create_game_session`.
/// Increments the global counter, inits the Game PDA, inits or refreshes
/// the PlayerProfile, consumes the escrow, asserts the postconditions
/// (Pending state + UNSET matchup). Returns the assigned `game_id`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_creation_state(
    counter: &mut GameCounter,
    game: &mut Game,
    player_profile: &mut PlayerProfile,
    escrow: &mut StakeEscrow,
    tournament_id: u64,
    player_key: Pubkey,
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
    now: i64,
    game_bump: u8,
    profile_bump: u8,
) -> Result<u64> {
    let game_id = counter.count;
    counter.count = counter
        .count
        .checked_add(1)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    init_game(
        game,
        game_id,
        tournament_id,
        player_key,
        stake_lamports,
        matchup_commitment,
        now,
        game_bump,
    );
    player_profile.init_if_new(player_key, tournament_id, profile_bump);
    require!(
        player_profile.tournament_id == tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );
    escrow.consumed = true;
    require!(
        game.state == GameState::Pending,
        CoordinationError::InvalidGameState
    );
    require!(
        game.matchup_type == MATCHUP_TYPE_UNSET,
        CoordinationError::InvalidGameState
    );
    Ok(game_id)
}

#[derive(Accounts)]
#[instruction(stake_lamports: u64, matchup_commitment: [u8; 32])]
pub struct CreateGame<'info> {
    #[account(
        init,
        payer = player,
        space = Game::SPACE,
        seeds = [b"game", game_counter.count.to_le_bytes().as_ref()],
        bump,
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [b"game_counter"],
        bump = game_counter.bump,
    )]
    pub game_counter: Account<'info, GameCounter>,
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
        seeds = [b"tournament", tournament.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    /// Matchmaker co-signs to attest the commitment is legitimate.
    /// Verified against GlobalConfig.matchmaker. Does not pay gas.
    pub matchmaker: Signer<'info>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}
