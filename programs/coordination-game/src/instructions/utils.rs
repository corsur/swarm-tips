use crate::errors::CoordinationError;
use crate::payoff::resolve_game;
use crate::state::{
    Game, GameState, PlayerProfile, Tournament, COMMIT_TIMEOUT_SLOTS, GUESS_SAME_TEAM,
    GUESS_UNREVEALED, MATCHUP_TYPE_UNSET, REVEAL_TIMEOUT_SLOTS,
};
use anchor_lang::prelude::*;

/// Transfer lamports directly between two program-owned or system accounts.
///
/// Used by reveal_guess, resolve_timeout, and claim_reward to move lamports
/// out of PDAs. Validates that `from` has sufficient balance before mutating.
pub fn transfer_lamports(from: &AccountInfo, to: &AccountInfo, lamports: u64) -> Result<()> {
    if lamports == 0 {
        return Ok(());
    }
    // Precondition: source account must have enough lamports
    require!(
        from.lamports() >= lamports,
        CoordinationError::InsufficientLamports
    );
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

/// Split result from `compute_treasury_split`.
pub struct TreasurySplit {
    pub treasury_share: u64,
    pub tournament_share: u64,
}

/// Compute the treasury/tournament split for pool gains.
///
/// Uses u128 intermediate to prevent overflow on large amounts.
/// Postcondition: `treasury_share + tournament_share == total`.
pub fn compute_treasury_split(total: u64, treasury_split_bps: u16) -> Result<TreasurySplit> {
    // Preconditions: total > 0 and bps within valid range
    require!(total > 0, CoordinationError::ArithmeticOverflow);
    require!(
        (crate::state::MIN_TREASURY_SPLIT_BPS..=crate::state::MAX_TREASURY_SPLIT_BPS)
            .contains(&treasury_split_bps),
        CoordinationError::InvalidTreasurySplitBps,
    );

    let total_128 = u128::from(total);
    let bps_128 = u128::from(treasury_split_bps);
    let treasury_128 = total_128
        .checked_mul(bps_128)
        .ok_or(CoordinationError::ArithmeticOverflow)?
        .checked_div(10_000)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    let treasury_share =
        u64::try_from(treasury_128).map_err(|_| CoordinationError::ArithmeticOverflow)?;
    let tournament_share = total
        .checked_sub(treasury_share)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    // Postcondition: shares sum to total
    require!(
        treasury_share
            .checked_add(tournament_share)
            .ok_or(CoordinationError::ArithmeticOverflow)?
            == total,
        CoordinationError::ArithmeticOverflow,
    );

    Ok(TreasurySplit {
        treasury_share,
        tournament_share,
    })
}

/// Outcome of `compute_finalization` — the values both `reveal_guess` and
/// `reveal_guess_session` need to commit. Computed once before any state
/// mutation; the caller hands the same values to `apply_finalize_effects`
/// (state) and `distribute_finalize_lamports` (interactions).
pub struct FinalizeOutcome {
    pub p1_return: u64,
    pub p2_return: u64,
    pub tournament_gain: u64,
    pub treasury_share: u64,
    pub tournament_share: u64,
    pub p1_won: bool,
    pub p2_won: bool,
    pub is_late_resolution: bool,
}

/// Pure-compute step shared by both reveal handlers. Computes the
/// returns, the win flags, and the treasury/tournament split so the
/// caller can apply effects and interactions independently.
pub fn compute_finalization(
    game: &Game,
    tournament_end_time: i64,
    treasury_split_bps: u16,
    now: i64,
) -> Result<FinalizeOutcome> {
    let is_late_resolution = now > tournament_end_time;
    let (p1_return, p2_return, tournament_gain) = if is_late_resolution {
        (game.stake_lamports, game.stake_lamports, 0u64)
    } else {
        let resolution = resolve_game(
            game.matchup_type,
            game.p1_guess,
            game.p2_guess,
            game.stake_lamports,
            game.first_committer,
        )?;
        (
            resolution.p1_return,
            resolution.p2_return,
            resolution.tournament_gain,
        )
    };
    // Determine wins. Late-resolved games don't count (refund with no scoring).
    let (p1_won, p2_won) = if is_late_resolution {
        (false, false)
    } else {
        let correct_guess = if game.matchup_type == 0 {
            GUESS_SAME_TEAM
        } else {
            crate::state::GUESS_DIFF_TEAM
        };
        let p1_correct = game.p1_guess == correct_guess;
        let p2_correct = game.p2_guess == correct_guess;
        if game.matchup_type == 1 && p1_correct && p2_correct {
            // Heterogeneous both-correct: only pot-winner earns a win.
            (p1_return > p2_return, p2_return > p1_return)
        } else {
            (p1_correct, p2_correct)
        }
    };
    // Compute treasury/tournament split exactly once.
    let (treasury_share, tournament_share) = if tournament_gain > 0 {
        let split = compute_treasury_split(tournament_gain, treasury_split_bps)?;
        (split.treasury_share, split.tournament_share)
    } else {
        (0, 0)
    };
    Ok(FinalizeOutcome {
        p1_return,
        p2_return,
        tournament_gain,
        treasury_share,
        tournament_share,
        p1_won,
        p2_won,
        is_late_resolution,
    })
}

/// Apply the state mutations for a finalized reveal: tournament
/// counters, player profile updates, and the Game state transition.
/// Pure CEI Effects step — no I/O.
pub fn apply_finalize_effects(
    tournament: &mut Tournament,
    p1_profile: &mut PlayerProfile,
    p2_profile: &mut PlayerProfile,
    game: &mut Game,
    outcome: &FinalizeOutcome,
    tournament_id: u64,
    now: i64,
) -> Result<()> {
    if !outcome.is_late_resolution {
        tournament.game_count = tournament
            .game_count
            .checked_add(1)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
        if outcome.tournament_share > 0 {
            tournament.prize_lamports = tournament
                .prize_lamports
                .checked_add(outcome.tournament_share)
                .ok_or(CoordinationError::ArithmeticOverflow)?;
        }
        require!(
            tournament.game_count >= 1,
            CoordinationError::ArithmeticOverflow,
        );
        p1_profile.update_after_game(outcome.p1_won, tournament_id)?;
        p2_profile.update_after_game(outcome.p2_won, tournament_id)?;
    }
    game.state = GameState::Resolved;
    game.resolved_at = now;
    require!(
        game.state == GameState::Resolved,
        CoordinationError::InvalidGameState
    );
    require!(game.resolved_at == now, CoordinationError::InvalidGameState);
    Ok(())
}

/// Lamport interactions for a finalized reveal. Caller passes the
/// `AccountInfo`s rather than typed accounts because the two reveal
/// variants have different `Accounts` structs.
pub fn distribute_finalize_lamports(
    game_info: &AccountInfo,
    p1_wallet: &AccountInfo,
    p2_wallet: &AccountInfo,
    treasury_info: &AccountInfo,
    tournament_info: &AccountInfo,
    outcome: &FinalizeOutcome,
) -> Result<()> {
    transfer_lamports(game_info, p1_wallet, outcome.p1_return)?;
    transfer_lamports(game_info, p2_wallet, outcome.p2_return)?;
    if outcome.tournament_gain > 0 {
        transfer_lamports(game_info, treasury_info, outcome.treasury_share)?;
        transfer_lamports(game_info, tournament_info, outcome.tournament_share)?;
    }
    Ok(())
}

/// Validate the end-of-tournament cutoff: no games may start within
/// `commit_timeout + reveal_timeout` of the tournament's end_time. Used
/// by both `create_game` and `create_game_session`.
pub fn validate_tournament_cutoff(now: i64, end_time: i64) -> Result<()> {
    let cutoff_slots = COMMIT_TIMEOUT_SLOTS
        .checked_add(REVEAL_TIMEOUT_SLOTS)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let slots_per_second: u64 = 2;
    let cutoff_seconds = cutoff_slots
        .checked_div(slots_per_second)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let cutoff_seconds_i64 =
        i64::try_from(cutoff_seconds).map_err(|_| CoordinationError::ArithmeticOverflow)?;
    let cutoff_timestamp = now
        .checked_add(cutoff_seconds_i64)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    require!(
        cutoff_timestamp < end_time,
        CoordinationError::OutsideTournamentWindow,
    );
    Ok(())
}

/// Initialize a freshly-allocated `Game` PDA with the same field set
/// regardless of which entry point (`create_game` or
/// `create_game_session`) created it. The two handlers were ~95%
/// duplicate before this helper was added — keep them in lockstep
/// here.
#[allow(clippy::too_many_arguments)]
pub fn init_game(
    game: &mut Game,
    game_id: u64,
    tournament_id: u64,
    player_one: Pubkey,
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
    now: i64,
    bump: u8,
) {
    game.game_id = game_id;
    game.tournament_id = tournament_id;
    game.player_one = player_one;
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
    game.matchup_commitment = matchup_commitment;
    game.matchup_type = MATCHUP_TYPE_UNSET;
    game.bump = bump;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_50_50_even_amount() {
        // 100_000_000 lamports at 5000 bps = 50/50
        let result = compute_treasury_split(100_000_000, 5_000).unwrap();
        assert_eq!(result.treasury_share, 50_000_000);
        assert_eq!(result.tournament_share, 50_000_000);
        assert_eq!(result.treasury_share + result.tournament_share, 100_000_000);
    }

    #[test]
    fn split_20_80_minimum_bps() {
        // 100_000_000 at 2000 bps (minimum) = 20% treasury, 80% tournament
        let result = compute_treasury_split(100_000_000, 2_000).unwrap();
        assert_eq!(result.treasury_share, 20_000_000);
        assert_eq!(result.tournament_share, 80_000_000);
    }

    #[test]
    fn split_80_20_maximum_bps() {
        // 100_000_000 at 8000 bps (maximum) = 80% treasury, 20% tournament
        let result = compute_treasury_split(100_000_000, 8_000).unwrap();
        assert_eq!(result.treasury_share, 80_000_000);
        assert_eq!(result.tournament_share, 20_000_000);
    }

    #[test]
    fn split_odd_amount_truncates_treasury_remainder_to_tournament() {
        // 3 lamports at 5000 bps: treasury = 3*5000/10000 = 1, tournament = 2
        let result = compute_treasury_split(3, 5_000).unwrap();
        assert_eq!(result.treasury_share, 1);
        assert_eq!(result.tournament_share, 2);
        assert_eq!(result.treasury_share + result.tournament_share, 3);
    }

    #[test]
    fn split_single_lamport() {
        // 1 lamport at 5000 bps: treasury = 0, tournament = 1
        let result = compute_treasury_split(1, 5_000).unwrap();
        assert_eq!(result.treasury_share, 0);
        assert_eq!(result.tournament_share, 1);
    }

    #[test]
    fn split_large_amount_no_overflow() {
        // u64::MAX / 2 to avoid overflow in the test assertion itself
        let large = u64::MAX / 2;
        let result = compute_treasury_split(large, 5_000).unwrap();
        assert_eq!(result.treasury_share + result.tournament_share, large);
    }

    #[test]
    fn split_rejects_zero_total() {
        let result = compute_treasury_split(0, 5_000);
        assert!(result.is_err());
    }

    #[test]
    fn split_rejects_bps_below_minimum() {
        let result = compute_treasury_split(100_000, 1_999);
        assert!(result.is_err());
    }

    #[test]
    fn split_rejects_bps_above_maximum() {
        let result = compute_treasury_split(100_000, 8_001);
        assert!(result.is_err());
    }

    #[test]
    fn split_typical_game_stake() {
        // 0.05 SOL = 50_000_000 lamports per player, 2S = 100_000_000
        // At default 5000 bps: 50M treasury, 50M tournament
        let two_s = 100_000_000u64;
        let result = compute_treasury_split(two_s, 5_000).unwrap();
        assert_eq!(result.treasury_share, 50_000_000);
        assert_eq!(result.tournament_share, 50_000_000);
    }
}
