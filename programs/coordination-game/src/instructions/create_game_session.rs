use crate::errors::CoordinationError;
use crate::events::GameCreated;
use crate::instructions::session_utils::validate_session_authority;
use crate::instructions::utils::transfer_lamports;
use crate::state::{
    Game, GameCounter, GameState, GlobalConfig, PlayerProfile, SessionAuthority, StakeEscrow,
    Tournament, COMMIT_TIMEOUT_SLOTS, FIXED_STAKE_LAMPORTS, GUESS_UNREVEALED, REVEAL_TIMEOUT_SLOTS,
};
use anchor_lang::prelude::*;

/// Session-delegated variant of `create_game`. The session key signs instead
/// of the player wallet. Matchmaker authority still required.
pub fn create_game_session(
    ctx: Context<CreateGameSession>,
    stake_lamports: u64,
    matchup_type: u8,
) -> Result<()> {
    validate_session_authority(
        &ctx.accounts.session_authority,
        &ctx.accounts.player.key(),
        &ctx.accounts.session_signer.key(),
    )?;

    require!(
        stake_lamports == FIXED_STAKE_LAMPORTS,
        CoordinationError::StakeMismatch
    );
    require!(matchup_type <= 1, CoordinationError::InvalidGameState);

    // Checks: matchmaker authority
    require!(
        ctx.accounts.matchmaker.key() == ctx.accounts.global_config.matchmaker,
        CoordinationError::NotMatchmaker
    );

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    require!(
        ctx.accounts.tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );

    // Checks: end-of-tournament cutoff
    let cutoff_slots = COMMIT_TIMEOUT_SLOTS
        .checked_add(REVEAL_TIMEOUT_SLOTS)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let slots_per_second: u64 = 2;
    let cutoff_seconds = cutoff_slots
        .checked_div(slots_per_second)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let cutoff_timestamp = now
        .checked_add(cutoff_seconds as i64)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    require!(
        cutoff_timestamp < ctx.accounts.tournament.end_time,
        CoordinationError::OutsideTournamentWindow,
    );

    // Validate the player's escrow has an unconsumed deposit
    let escrow = &ctx.accounts.escrow;
    require!(
        escrow.validate_for_game(
            &ctx.accounts.player.key(),
            ctx.accounts.tournament.tournament_id
        ),
        CoordinationError::EscrowInvalid,
    );

    // Assign game_id from counter, then increment
    let counter = &mut ctx.accounts.game_counter;
    let game_id = counter.count;
    counter.count = counter
        .count
        .checked_add(1)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    let game = &mut ctx.accounts.game;
    game.game_id = game_id;
    game.tournament_id = ctx.accounts.tournament.tournament_id;
    game.player_one = ctx.accounts.player.key();
    game.player_two = Pubkey::default();
    game.state = GameState::Pending;
    game.stake_lamports = stake_lamports;
    game.p1_commit = [0u8; 32];
    game.p2_commit = [0u8; 32];
    game.p1_guess = GUESS_UNREVEALED;
    game.p2_guess = GUESS_UNREVEALED;
    game.first_committer = 0;
    game.p1_commit_slot = 0;
    game.p2_commit_slot = 0;
    game.commit_timeout_slots = COMMIT_TIMEOUT_SLOTS;
    game.created_at = now;
    game.resolved_at = 0;
    game.activated_at_slot = 0;
    game.matchup_type = matchup_type;
    game.bump = ctx.bumps.game;

    // Init player profile if needed
    let tournament_id = ctx.accounts.tournament.tournament_id;
    ctx.accounts.player_profile.init_if_new(
        ctx.accounts.player.key(),
        tournament_id,
        ctx.bumps.player_profile,
    );
    require!(
        ctx.accounts.player_profile.tournament_id == tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );

    // Mark escrow as consumed before transferring (CEI)
    ctx.accounts.escrow.consumed = true;

    // Postconditions
    require!(
        game.state == GameState::Pending,
        CoordinationError::InvalidGameState
    );
    require!(
        game.player_one == ctx.accounts.player.key(),
        CoordinationError::InvalidGameState
    );

    // Transfer stake from escrow PDA to game PDA
    transfer_lamports(
        &ctx.accounts.escrow.to_account_info(),
        &ctx.accounts.game.to_account_info(),
        stake_lamports,
    )?;

    emit!(GameCreated {
        game_id,
        tournament_id: ctx.accounts.tournament.tournament_id,
        player_one: ctx.accounts.player.key(),
        stake_lamports,
    });
    Ok(())
}

#[derive(Accounts)]
#[instruction(stake_lamports: u64, matchup_type: u8)]
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
    pub tournament: Account<'info, Tournament>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    pub matchmaker: Signer<'info>,
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
