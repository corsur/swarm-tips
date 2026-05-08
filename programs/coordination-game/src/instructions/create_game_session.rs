use crate::errors::CoordinationError;
use crate::events::GameCreated;
use crate::instructions::session_utils::validate_session_authority;
use crate::instructions::utils::{init_game, transfer_lamports, validate_tournament_cutoff};
use crate::state::{
    Game, GameCounter, GameState, GlobalConfig, PlayerProfile, SessionAuthority, StakeEscrow,
    Tournament, FIXED_STAKE_LAMPORTS, MATCHUP_TYPE_UNSET,
};
use anchor_lang::prelude::*;

/// Session-delegated variant of `create_game`. Player creates the game via a
/// session key; the matchmaker wallet is verified against GlobalConfig but does
/// not need to sign (session authority proves matchmaker delegation).
pub fn create_game_session(
    ctx: Context<CreateGameSession>,
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let player_key = ctx.accounts.player.key();
    let tournament_id = ctx.accounts.tournament.tournament_id;
    // Checks
    validate_session_authority(
        &ctx.accounts.session_authority,
        &ctx.accounts.matchmaker_wallet.key(),
        &ctx.accounts.session_signer.key(),
    )?;
    validate_create_session_inputs(
        stake_lamports,
        matchup_commitment,
        ctx.accounts.matchmaker_wallet.key(),
        ctx.accounts.global_config.matchmaker,
        &ctx.accounts.tournament,
        &ctx.accounts.escrow,
        &player_key,
        tournament_id,
        now,
    )?;
    // Effects: assign game_id, init Game + PlayerProfile, consume escrow.
    let counter = &mut ctx.accounts.game_counter;
    let game_id = counter.count;
    counter.count = counter
        .count
        .checked_add(1)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    init_game(
        &mut ctx.accounts.game,
        game_id,
        tournament_id,
        player_key,
        stake_lamports,
        matchup_commitment,
        now,
        ctx.bumps.game,
    );
    ctx.accounts
        .player_profile
        .init_if_new(player_key, tournament_id, ctx.bumps.player_profile);
    require!(
        ctx.accounts.player_profile.tournament_id == tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );
    ctx.accounts.escrow.consumed = true;
    // Postconditions
    require!(
        ctx.accounts.game.state == GameState::Pending,
        CoordinationError::InvalidGameState
    );
    require!(
        ctx.accounts.game.matchup_type == MATCHUP_TYPE_UNSET,
        CoordinationError::InvalidGameState
    );
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

/// Same rule set as `create_game::validate_create_inputs` — duplicated
/// only because the session variant uses `matchmaker_wallet` (an
/// `UncheckedAccount`) rather than `matchmaker` (a `Signer`). The body
/// is otherwise identical.
#[allow(clippy::too_many_arguments)]
fn validate_create_session_inputs(
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

#[derive(Accounts)]
#[instruction(stake_lamports: u64, matchup_commitment: [u8; 32])]
pub struct CreateGameSession<'info> {
    #[account(
        init,
        payer = session_signer,
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
        seeds = [b"tournament", tournament.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    /// CHECK: The player wallet. Not a signer — the session key signs instead.
    /// Verified via escrow seeds and session_authority.
    pub player: UncheckedAccount<'info>,
    /// CHECK: The matchmaker wallet. Verified against global_config.matchmaker.
    pub matchmaker_wallet: UncheckedAccount<'info>,
    #[account(
        seeds = [
            b"game_session",
            matchmaker_wallet.key().as_ref(),
            session_signer.key().as_ref(),
        ],
        bump = session_authority.bump,
    )]
    pub session_authority: Account<'info, SessionAuthority>,
    #[account(mut)]
    pub session_signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
