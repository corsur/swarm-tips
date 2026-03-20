use anchor_lang::prelude::*;
use crate::errors::CoordinationError;
use crate::events::TimeoutSlash;
use crate::state::{Game, GameState, PlayerProfile, Tournament, REVEAL_TIMEOUT_SLOTS};

pub fn resolve_timeout(ctx: Context<ResolveTimeout>) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(
        game.state == GameState::Committing || game.state == GameState::Revealing,
        CoordinationError::InvalidGameState,
    );

    let current_slot = Clock::get()?.slot;
    let now = Clock::get()?.unix_timestamp;

    let outcome = find_timeout(game, current_slot)?;

    let (tournament_gain, slashed_player) = match outcome {
        TimeoutOutcome::OneWinner { slashed_player, winner_is_p1 } => {
            // Slash the non-participating player; return the winner's stake
            transfer_from_game(
                &ctx.accounts.game.to_account_info(),
                &ctx.accounts.tournament.to_account_info(),
                game.stake_lamports,
            )?;
            let winner_account = if winner_is_p1 {
                ctx.accounts.player_one_wallet.to_account_info()
            } else {
                ctx.accounts.player_two_wallet.to_account_info()
            };
            transfer_from_game(
                &ctx.accounts.game.to_account_info(),
                &winner_account,
                game.stake_lamports,
            )?;
            let tournament_id = ctx.accounts.tournament.tournament_id;
            update_profile(&mut ctx.accounts.p1_profile, winner_is_p1, tournament_id)?;
            update_profile(&mut ctx.accounts.p2_profile, !winner_is_p1, tournament_id)?;
            (game.stake_lamports, slashed_player)
        }
        TimeoutOutcome::BothForfeited => {
            // Neither player revealed — both stakes go to tournament, no winner
            let both_stakes = game.stake_lamports
                .checked_mul(2)
                .ok_or(CoordinationError::ArithmeticOverflow)?;
            transfer_from_game(
                &ctx.accounts.game.to_account_info(),
                &ctx.accounts.tournament.to_account_info(),
                both_stakes,
            )?;
            let tournament_id = ctx.accounts.tournament.tournament_id;
            update_profile(&mut ctx.accounts.p1_profile, false, tournament_id)?;
            update_profile(&mut ctx.accounts.p2_profile, false, tournament_id)?;
            // Report player_one as the canonical slashed address in the event;
            // both were slashed but the event only carries one field.
            (both_stakes, game.player_one)
        }
    };

    // Update tournament
    let tournament = &mut ctx.accounts.tournament;
    tournament.prize_lamports = tournament.prize_lamports
        .checked_add(tournament_gain)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    tournament.game_count = tournament.game_count
        .checked_add(1)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    let game = &mut ctx.accounts.game;
    game.state = GameState::Resolved;
    game.resolved_at = now;

    // Postcondition: game must be resolved and timestamped
    require!(game.state == GameState::Resolved, CoordinationError::InvalidGameState);
    require!(game.resolved_at == now, CoordinationError::InvalidGameState);

    emit!(TimeoutSlash {
        game_id: game.game_id,
        slashed_player,
        slash_amount: tournament_gain,
    });
    Ok(())
}

enum TimeoutOutcome {
    /// One player participated; the other forfeited.
    OneWinner { slashed_player: Pubkey, winner_is_p1: bool },
    /// Both players failed to reveal — both stakes forfeit to tournament, no winner.
    BothForfeited,
}

fn find_timeout(game: &Game, current_slot: u64) -> Result<TimeoutOutcome> {
    match game.state {
        GameState::Committing => {
            // One player committed, the other hasn't — the non-committer timed out
            let p1_committed = game.p1_commit != [0u8; 32];
            let commit_slot = if p1_committed {
                game.p1_commit_slot
            } else {
                game.p2_commit_slot
            };
            require!(
                current_slot >= commit_slot
                    .checked_add(game.commit_timeout_slots)
                    .ok_or(CoordinationError::ArithmeticOverflow)?,
                CoordinationError::TimeoutNotElapsed,
            );
            if p1_committed {
                Ok(TimeoutOutcome::OneWinner { slashed_player: game.player_two, winner_is_p1: true })
            } else {
                Ok(TimeoutOutcome::OneWinner { slashed_player: game.player_one, winner_is_p1: false })
            }
        }
        GameState::Revealing => {
            // Both committed; one or both haven't revealed within REVEAL_TIMEOUT_SLOTS
            let p1_revealed = game.p1_guess != crate::state::GUESS_UNREVEALED;
            let p2_revealed = game.p2_guess != crate::state::GUESS_UNREVEALED;

            // Use the later commit slot as the timeout anchor — both had to commit
            // before revealing, so the clock starts from the last commit
            let anchor_slot = game.p1_commit_slot.max(game.p2_commit_slot);
            let deadline = anchor_slot
                .checked_add(REVEAL_TIMEOUT_SLOTS)
                .ok_or(CoordinationError::ArithmeticOverflow)?;
            require!(current_slot >= deadline, CoordinationError::TimeoutNotElapsed);

            match (p1_revealed, p2_revealed) {
                (true, false) => Ok(TimeoutOutcome::OneWinner { slashed_player: game.player_two, winner_is_p1: true }),
                (false, true) => Ok(TimeoutOutcome::OneWinner { slashed_player: game.player_one, winner_is_p1: false }),
                (false, false) => Ok(TimeoutOutcome::BothForfeited),
                (true, true) => {
                    // Both revealed — should have been resolved already
                    err!(CoordinationError::InvalidGameState)
                }
            }
        }
        _ => err!(CoordinationError::InvalidGameState),
    }
}

fn update_profile(profile: &mut PlayerProfile, won: bool, tournament_id: u64) -> Result<()> {
    require!(
        profile.tournament_id == tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );
    if won {
        profile.wins = profile.wins
            .checked_add(1)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
    }
    profile.total_games = profile.total_games
        .checked_add(1)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    profile.score = PlayerProfile::compute_score(profile.wins, profile.total_games)?;
    // Postcondition: wins must never exceed total_games
    require!(
        profile.wins <= profile.total_games,
        CoordinationError::ArithmeticOverflow,
    );
    Ok(())
}

fn transfer_from_game(from: &AccountInfo, to: &AccountInfo, lamports: u64) -> Result<()> {
    if lamports == 0 {
        return Ok(());
    }
    **from.try_borrow_mut_lamports()? = from
        .lamports()
        .checked_sub(lamports)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    **to.try_borrow_mut_lamports()? = to
        .lamports()
        .checked_add(lamports)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    Ok(())
}

#[derive(Accounts)]
pub struct ResolveTimeout<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [
            b"player",
            tournament.tournament_id.to_le_bytes().as_ref(),
            game.player_one.as_ref(),
        ],
        bump = p1_profile.bump,
        constraint = p1_profile.wallet == game.player_one,
    )]
    pub p1_profile: Account<'info, PlayerProfile>,
    #[account(
        mut,
        seeds = [
            b"player",
            tournament.tournament_id.to_le_bytes().as_ref(),
            game.player_two.as_ref(),
        ],
        bump = p2_profile.bump,
        constraint = p2_profile.wallet == game.player_two,
    )]
    pub p2_profile: Account<'info, PlayerProfile>,
    #[account(
        mut,
        seeds = [b"tournament", game.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    /// CHECK: Verified by address constraint against game.player_one
    #[account(mut, address = game.player_one)]
    pub player_one_wallet: UncheckedAccount<'info>,
    /// CHECK: Verified by address constraint against game.player_two
    #[account(mut, address = game.player_two)]
    pub player_two_wallet: UncheckedAccount<'info>,
    /// Caller receives no prize but pays the transaction fee; rent reclaim via close_game
    pub caller: Signer<'info>,
}
