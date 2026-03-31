use crate::errors::CoordinationError;
use crate::events::TimeoutSlash;
use crate::instructions::utils::{compute_treasury_split, transfer_lamports};
use crate::state::{
    Game, GameState, GlobalConfig, PlayerProfile, Tournament, REVEAL_TIMEOUT_SLOTS,
};
use anchor_lang::prelude::*;

pub fn resolve_timeout(ctx: Context<ResolveTimeout>) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(
        game.state == GameState::Active
            || game.state == GameState::Committing
            || game.state == GameState::Revealing,
        CoordinationError::InvalidGameState,
    );

    let current_slot = Clock::get()?.slot;
    let now = Clock::get()?.unix_timestamp;
    let outcome = find_timeout(game, current_slot)?;

    let tournament_id = ctx.accounts.tournament.tournament_id;
    let treasury_split_bps = ctx.accounts.global_config.treasury_split_bps;

    // Compute outcome values and capture AccountInfo handles before mutable borrows
    let stake_lamports = game.stake_lamports;
    let both_stakes = stake_lamports
        .checked_mul(2)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    //  ┌──────────────────────────────────────────────────────────────────┐
    //  │ OneWinner: winner gets full pot (2S), tournament gets 0.        │
    //  │   Anti-griefing: non-participant forfeits, revealer/committer   │
    //  │   is fully compensated.                                        │
    //  │                                                                │
    //  │ BothForfeited: both lose. Pool gain = 2S split via treasury    │
    //  │   split bps between treasury and tournament.                   │
    //  └──────────────────────────────────────────────────────────────────┘
    let (pool_gain, slashed_player, p1_won, p2_won, winner_wallet) = match outcome {
        TimeoutOutcome::OneWinner {
            slashed_player,
            winner_is_p1,
        } => {
            let winner_wallet = if winner_is_p1 {
                Some(ctx.accounts.player_one_wallet.to_account_info())
            } else {
                Some(ctx.accounts.player_two_wallet.to_account_info())
            };
            // Winner gets full pot (2S); tournament/treasury get 0
            (
                0u64,
                slashed_player,
                winner_is_p1,
                !winner_is_p1,
                winner_wallet,
            )
        }
        TimeoutOutcome::BothForfeited => {
            // Report player_one as canonical slashed address; both were slashed
            (both_stakes, game.player_one, false, false, None)
        }
    };
    // `game` borrow ends here (NLL — last use above)

    let game_info = ctx.accounts.game.to_account_info();
    let tournament_info = ctx.accounts.tournament.to_account_info();
    let treasury_info = ctx.accounts.treasury.to_account_info();

    // Compute treasury/tournament split for pool gain
    let (treasury_share, tournament_share) = if pool_gain > 0 {
        let split = compute_treasury_split(pool_gain, treasury_split_bps)?;
        (split.treasury_share, split.tournament_share)
    } else {
        (0u64, 0u64)
    };

    // Effects: apply all state mutations before any lamport transfers
    ctx.accounts
        .p1_profile
        .update_after_game(p1_won, tournament_id)?;
    ctx.accounts
        .p2_profile
        .update_after_game(p2_won, tournament_id)?;

    // Always increment game_count for ALL resolved games
    ctx.accounts.tournament.game_count = ctx
        .accounts
        .tournament
        .game_count
        .checked_add(1)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    if tournament_share > 0 {
        ctx.accounts.tournament.prize_lamports = ctx
            .accounts
            .tournament
            .prize_lamports
            .checked_add(tournament_share)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
    }

    let game_id = ctx.accounts.game.game_id;
    ctx.accounts.game.state = GameState::Resolved;
    ctx.accounts.game.resolved_at = now;

    // Postconditions: game must be resolved and timestamped
    require!(
        ctx.accounts.game.state == GameState::Resolved,
        CoordinationError::InvalidGameState
    );
    require!(
        ctx.accounts.game.resolved_at == now,
        CoordinationError::InvalidGameState
    );

    // Interactions: lamport transfers after all state is committed
    if let Some(winner) = winner_wallet {
        // Winner gets the full pot (2S)
        transfer_lamports(&game_info, &winner, both_stakes)?;
    }
    if treasury_share > 0 {
        transfer_lamports(&game_info, &treasury_info, treasury_share)?;
    }
    if tournament_share > 0 {
        transfer_lamports(&game_info, &tournament_info, tournament_share)?;
    }

    emit!(TimeoutSlash {
        game_id,
        slashed_player,
        slash_amount: pool_gain,
    });
    Ok(())
}

enum TimeoutOutcome {
    /// One player participated; the other forfeited.
    OneWinner {
        slashed_player: Pubkey,
        winner_is_p1: bool,
    },
    /// Both players failed to reveal — both stakes forfeit to tournament, no winner.
    BothForfeited,
}

fn find_timeout(game: &Game, current_slot: u64) -> Result<TimeoutOutcome> {
    match game.state {
        GameState::Active => find_active_timeout(game, current_slot),
        GameState::Committing => find_committing_timeout(game, current_slot),
        GameState::Revealing => find_revealing_timeout(game, current_slot),
        _ => err!(CoordinationError::InvalidGameState),
    }
}

/// Neither player has committed within the commit window after game activation.
/// Both players forfeit — both stakes go to pool/treasury split.
fn find_active_timeout(game: &Game, current_slot: u64) -> Result<TimeoutOutcome> {
    // Precondition: game is Active (neither player committed)
    require!(
        game.p1_commit == [0u8; 32] && game.p2_commit == [0u8; 32],
        CoordinationError::InvalidGameState,
    );

    let deadline = game
        .activated_at_slot
        .checked_add(game.commit_timeout_slots)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    require!(
        current_slot >= deadline,
        CoordinationError::TimeoutNotElapsed
    );

    Ok(TimeoutOutcome::BothForfeited)
}

/// One player committed; the other hasn't within the commit window.
/// The non-committer is slashed; the committer wins.
fn find_committing_timeout(game: &Game, current_slot: u64) -> Result<TimeoutOutcome> {
    let p1_committed = game.p1_commit != [0u8; 32];
    let commit_slot = if p1_committed {
        game.p1_commit_slot
    } else {
        game.p2_commit_slot
    };
    require!(
        current_slot
            >= commit_slot
                .checked_add(game.commit_timeout_slots)
                .ok_or(CoordinationError::ArithmeticOverflow)?,
        CoordinationError::TimeoutNotElapsed,
    );
    if p1_committed {
        Ok(TimeoutOutcome::OneWinner {
            slashed_player: game.player_two,
            winner_is_p1: true,
        })
    } else {
        Ok(TimeoutOutcome::OneWinner {
            slashed_player: game.player_one,
            winner_is_p1: false,
        })
    }
}

/// Both players committed; one or both failed to reveal within the reveal window.
/// The clock starts from the later of the two commit slots.
fn find_revealing_timeout(game: &Game, current_slot: u64) -> Result<TimeoutOutcome> {
    let p1_revealed = game.p1_guess != crate::state::GUESS_UNREVEALED;
    let p2_revealed = game.p2_guess != crate::state::GUESS_UNREVEALED;

    let anchor_slot = game.p1_commit_slot.max(game.p2_commit_slot);
    let deadline = anchor_slot
        .checked_add(REVEAL_TIMEOUT_SLOTS)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    require!(
        current_slot >= deadline,
        CoordinationError::TimeoutNotElapsed
    );

    match (p1_revealed, p2_revealed) {
        (true, false) => Ok(TimeoutOutcome::OneWinner {
            slashed_player: game.player_two,
            winner_is_p1: true,
        }),
        (false, true) => Ok(TimeoutOutcome::OneWinner {
            slashed_player: game.player_one,
            winner_is_p1: false,
        }),
        (false, false) => Ok(TimeoutOutcome::BothForfeited),
        (true, true) => {
            // Both revealed — reveal_guess should have resolved this already
            err!(CoordinationError::InvalidGameState)
        }
    }
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
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    /// CHECK: DAO treasury — validated against global_config.treasury
    #[account(mut, address = global_config.treasury)]
    pub treasury: UncheckedAccount<'info>,
    /// CHECK: Verified by address constraint against game.player_one
    #[account(mut, address = game.player_one)]
    pub player_one_wallet: UncheckedAccount<'info>,
    /// CHECK: Verified by address constraint against game.player_two
    #[account(mut, address = game.player_two)]
    pub player_two_wallet: UncheckedAccount<'info>,
    /// Caller receives no prize but pays the transaction fee; rent reclaim via close_game
    pub caller: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{GameState, COMMIT_TIMEOUT_SLOTS, GUESS_UNREVEALED, REVEAL_TIMEOUT_SLOTS};
    use anchor_lang::prelude::Pubkey;

    /// Build a Game with sensible defaults; tests override the fields they care about.
    fn base_game() -> Game {
        Game {
            game_id: 1,
            tournament_id: 1,
            player_one: Pubkey::new_unique(),
            player_two: Pubkey::new_unique(),
            state: GameState::Active,
            stake_lamports: 50_000_000,
            p1_commit: [0u8; 32],
            p2_commit: [0u8; 32],
            p1_guess: GUESS_UNREVEALED,
            p2_guess: GUESS_UNREVEALED,
            first_committer: 0,
            p1_commit_slot: 0,
            p2_commit_slot: 0,
            commit_timeout_slots: COMMIT_TIMEOUT_SLOTS,
            created_at: 0,
            resolved_at: 0,
            activated_at_slot: 100,
            matchup_commitment: [0u8; 32],
            matchup_type: 255,
            bump: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Active timeout: neither player committed
    // -----------------------------------------------------------------------

    #[test]
    fn active_timeout_elapsed_both_forfeit() {
        let game = base_game();
        // Deadline = activated_at_slot + commit_timeout_slots = 100 + 7200 = 7300
        let current_slot = game.activated_at_slot + game.commit_timeout_slots;
        let result = find_active_timeout(&game, current_slot).unwrap();
        assert!(
            matches!(result, TimeoutOutcome::BothForfeited),
            "both players should forfeit when neither committed and timeout elapsed"
        );
    }

    #[test]
    fn active_timeout_well_past_deadline() {
        let game = base_game();
        let current_slot = game.activated_at_slot + game.commit_timeout_slots + 10_000;
        let result = find_active_timeout(&game, current_slot).unwrap();
        assert!(matches!(result, TimeoutOutcome::BothForfeited));
    }

    #[test]
    fn active_timeout_not_elapsed_errors() {
        let game = base_game();
        // One slot before the deadline
        let current_slot = game.activated_at_slot + game.commit_timeout_slots - 1;
        let result = find_active_timeout(&game, current_slot);
        assert!(result.is_err(), "should error when timeout has not elapsed");
    }

    // -----------------------------------------------------------------------
    // Committing timeout: one player committed, the other has not
    // -----------------------------------------------------------------------

    #[test]
    fn committing_timeout_p1_committed_elapsed_p1_wins() {
        let mut game = base_game();
        game.state = GameState::Committing;
        game.p1_commit = [1u8; 32]; // p1 committed
        game.p1_commit_slot = 200;
        // Deadline = p1_commit_slot + commit_timeout_slots = 200 + 7200 = 7400
        let current_slot = game.p1_commit_slot + game.commit_timeout_slots;
        let result = find_committing_timeout(&game, current_slot).unwrap();
        match result {
            TimeoutOutcome::OneWinner {
                slashed_player,
                winner_is_p1,
            } => {
                assert!(winner_is_p1, "p1 should win (p1 committed)");
                assert_eq!(slashed_player, game.player_two, "p2 should be slashed");
            }
            TimeoutOutcome::BothForfeited => {
                panic!("expected OneWinner, got BothForfeited");
            }
        }
    }

    #[test]
    fn committing_timeout_p2_committed_elapsed_p2_wins() {
        let mut game = base_game();
        game.state = GameState::Committing;
        game.p2_commit = [2u8; 32]; // p2 committed
        game.p2_commit_slot = 300;
        let current_slot = game.p2_commit_slot + game.commit_timeout_slots;
        let result = find_committing_timeout(&game, current_slot).unwrap();
        match result {
            TimeoutOutcome::OneWinner {
                slashed_player,
                winner_is_p1,
            } => {
                assert!(!winner_is_p1, "p2 should win (p2 committed)");
                assert_eq!(slashed_player, game.player_one, "p1 should be slashed");
            }
            TimeoutOutcome::BothForfeited => {
                panic!("expected OneWinner, got BothForfeited");
            }
        }
    }

    #[test]
    fn committing_timeout_not_elapsed_errors() {
        let mut game = base_game();
        game.state = GameState::Committing;
        game.p1_commit = [1u8; 32];
        game.p1_commit_slot = 200;
        // One slot before deadline
        let current_slot = game.p1_commit_slot + game.commit_timeout_slots - 1;
        let result = find_committing_timeout(&game, current_slot);
        assert!(
            result.is_err(),
            "should error when committing timeout has not elapsed"
        );
    }

    // -----------------------------------------------------------------------
    // Revealing timeout: both committed, one or neither revealed
    // -----------------------------------------------------------------------

    #[test]
    fn revealing_timeout_p1_revealed_elapsed_p1_wins() {
        let mut game = base_game();
        game.state = GameState::Revealing;
        game.p1_commit = [1u8; 32];
        game.p2_commit = [2u8; 32];
        game.p1_commit_slot = 200;
        game.p2_commit_slot = 250;
        game.p1_guess = 0; // p1 revealed
                           // p2_guess stays GUESS_UNREVEALED
                           // anchor_slot = max(200, 250) = 250; deadline = 250 + REVEAL_TIMEOUT_SLOTS
        let current_slot = 250 + REVEAL_TIMEOUT_SLOTS;
        let result = find_revealing_timeout(&game, current_slot).unwrap();
        match result {
            TimeoutOutcome::OneWinner {
                slashed_player,
                winner_is_p1,
            } => {
                assert!(winner_is_p1, "p1 should win (p1 revealed)");
                assert_eq!(slashed_player, game.player_two, "p2 should be slashed");
            }
            TimeoutOutcome::BothForfeited => {
                panic!("expected OneWinner, got BothForfeited");
            }
        }
    }

    #[test]
    fn revealing_timeout_p2_revealed_elapsed_p2_wins() {
        let mut game = base_game();
        game.state = GameState::Revealing;
        game.p1_commit = [1u8; 32];
        game.p2_commit = [2u8; 32];
        game.p1_commit_slot = 300;
        game.p2_commit_slot = 280;
        // p1_guess stays GUESS_UNREVEALED
        game.p2_guess = 1; // p2 revealed
                           // anchor_slot = max(300, 280) = 300; deadline = 300 + REVEAL_TIMEOUT_SLOTS
        let current_slot = 300 + REVEAL_TIMEOUT_SLOTS;
        let result = find_revealing_timeout(&game, current_slot).unwrap();
        match result {
            TimeoutOutcome::OneWinner {
                slashed_player,
                winner_is_p1,
            } => {
                assert!(!winner_is_p1, "p2 should win (p2 revealed)");
                assert_eq!(slashed_player, game.player_one, "p1 should be slashed");
            }
            TimeoutOutcome::BothForfeited => {
                panic!("expected OneWinner, got BothForfeited");
            }
        }
    }

    #[test]
    fn revealing_timeout_neither_revealed_elapsed_both_forfeit() {
        let mut game = base_game();
        game.state = GameState::Revealing;
        game.p1_commit = [1u8; 32];
        game.p2_commit = [2u8; 32];
        game.p1_commit_slot = 100;
        game.p2_commit_slot = 150;
        // Neither revealed — both guesses stay at GUESS_UNREVEALED
        // anchor_slot = max(100, 150) = 150; deadline = 150 + REVEAL_TIMEOUT_SLOTS
        let current_slot = 150 + REVEAL_TIMEOUT_SLOTS;
        let result = find_revealing_timeout(&game, current_slot).unwrap();
        assert!(
            matches!(result, TimeoutOutcome::BothForfeited),
            "both should forfeit when neither revealed and timeout elapsed"
        );
    }

    #[test]
    fn revealing_timeout_not_elapsed_errors() {
        let mut game = base_game();
        game.state = GameState::Revealing;
        game.p1_commit = [1u8; 32];
        game.p2_commit = [2u8; 32];
        game.p1_commit_slot = 100;
        game.p2_commit_slot = 150;
        game.p1_guess = 0; // p1 revealed
                           // One slot before deadline
        let current_slot = 150 + REVEAL_TIMEOUT_SLOTS - 1;
        let result = find_revealing_timeout(&game, current_slot);
        assert!(
            result.is_err(),
            "should error when reveal timeout has not elapsed"
        );
    }

    #[test]
    fn revealing_timeout_both_revealed_errors() {
        // Both revealed should have been resolved by reveal_guess already.
        let mut game = base_game();
        game.state = GameState::Revealing;
        game.p1_commit = [1u8; 32];
        game.p2_commit = [2u8; 32];
        game.p1_commit_slot = 100;
        game.p2_commit_slot = 150;
        game.p1_guess = 0;
        game.p2_guess = 1;
        let current_slot = 150 + REVEAL_TIMEOUT_SLOTS;
        let result = find_revealing_timeout(&game, current_slot);
        assert!(
            result.is_err(),
            "should error when both players have already revealed"
        );
    }

    // -----------------------------------------------------------------------
    // find_timeout router
    // -----------------------------------------------------------------------

    #[test]
    fn find_timeout_routes_active() {
        let game = base_game();
        let current_slot = game.activated_at_slot + game.commit_timeout_slots;
        let result = find_timeout(&game, current_slot).unwrap();
        assert!(matches!(result, TimeoutOutcome::BothForfeited));
    }

    #[test]
    fn find_timeout_routes_committing() {
        let mut game = base_game();
        game.state = GameState::Committing;
        game.p1_commit = [1u8; 32];
        game.p1_commit_slot = 200;
        let current_slot = game.p1_commit_slot + game.commit_timeout_slots;
        let result = find_timeout(&game, current_slot).unwrap();
        assert!(matches!(
            result,
            TimeoutOutcome::OneWinner {
                winner_is_p1: true,
                ..
            }
        ));
    }

    #[test]
    fn find_timeout_routes_revealing() {
        let mut game = base_game();
        game.state = GameState::Revealing;
        game.p1_commit = [1u8; 32];
        game.p2_commit = [2u8; 32];
        game.p1_commit_slot = 100;
        game.p2_commit_slot = 150;
        game.p1_guess = 0; // only p1 revealed
        let current_slot = 150 + REVEAL_TIMEOUT_SLOTS;
        let result = find_timeout(&game, current_slot).unwrap();
        assert!(matches!(
            result,
            TimeoutOutcome::OneWinner {
                winner_is_p1: true,
                ..
            }
        ));
    }

    #[test]
    fn find_timeout_rejects_resolved_state() {
        let mut game = base_game();
        game.state = GameState::Resolved;
        let result = find_timeout(&game, 99999);
        assert!(result.is_err(), "should error for Resolved state");
    }

    #[test]
    fn find_timeout_rejects_pending_state() {
        let mut game = base_game();
        game.state = GameState::Pending;
        let result = find_timeout(&game, 99999);
        assert!(result.is_err(), "should error for Pending state");
    }
}
