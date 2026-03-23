use crate::errors::CoordinationError;
use crate::events::GameCreated;
use crate::instructions::utils::transfer_lamports;
use crate::state::{
    Game, GameCounter, GameState, PlayerProfile, StakeEscrow, Tournament, COMMIT_TIMEOUT_SLOTS,
    FIXED_STAKE_LAMPORTS, GUESS_UNREVEALED,
};
use anchor_lang::prelude::*;

pub fn create_game(ctx: Context<CreateGame>, stake_lamports: u64, matchup_type: u8) -> Result<()> {
    require!(
        stake_lamports == FIXED_STAKE_LAMPORTS,
        CoordinationError::StakeMismatch
    );
    require!(matchup_type <= 1, CoordinationError::InvalidGameState);

    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.tournament.is_active(now),
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
    game.matchup_type = matchup_type;
    game.bump = ctx.bumps.game;

    // Init player profile if needed — player pays for their own account
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
    pub tournament: Account<'info, Tournament>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}
