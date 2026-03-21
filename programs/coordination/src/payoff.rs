use crate::errors::CoordinationError;
use anchor_lang::prelude::*;

pub struct Resolution {
    pub p1_return: u64,
    pub p2_return: u64,
    pub tournament_gain: u64,
}

/// Computes payoffs for a same-team (homogenous) matchup.
///
/// Correct guess = GUESS_SAME_TEAM (0), since both players are on the same team.
///
/// Payoffs:
///   Both correct: each receives 90% of stake; house takes 10% from each (20% total)
///   At least one wrong: both forfeit; house takes 100% from each (200% total)
///
/// Invariant: p1_return + p2_return + tournament_gain == 2 * stake_lamports
pub fn resolve_homogenous(p1_guess: u8, p2_guess: u8, stake_lamports: u64) -> Result<Resolution> {
    // Invariant: stake_lamports must be nonzero
    require!(stake_lamports > 0, CoordinationError::ArithmeticOverflow);

    let two_stakes = stake_lamports
        .checked_mul(2)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    let both_correct =
        p1_guess == crate::state::GUESS_SAME_TEAM && p2_guess == crate::state::GUESS_SAME_TEAM;

    if both_correct {
        // 90% return: stake * 9 / 10
        // Fee computed as remainder to avoid rounding loss
        let each_return = stake_lamports
            .checked_mul(9)
            .and_then(|v| v.checked_div(10))
            .ok_or(CoordinationError::ArithmeticOverflow)?;
        let fee = stake_lamports
            .checked_sub(each_return)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
        let tournament_gain = fee
            .checked_mul(2)
            .ok_or(CoordinationError::ArithmeticOverflow)?;

        // Assert invariant
        let total = each_return
            .checked_add(each_return)
            .and_then(|v| v.checked_add(tournament_gain))
            .ok_or(CoordinationError::ArithmeticOverflow)?;
        require!(total == two_stakes, CoordinationError::ArithmeticOverflow);

        Ok(Resolution {
            p1_return: each_return,
            p2_return: each_return,
            tournament_gain,
        })
    } else {
        // Both forfeit — full 2× stake goes to tournament
        let resolution = Resolution {
            p1_return: 0,
            p2_return: 0,
            tournament_gain: two_stakes,
        };
        // Postcondition: lamports conserved
        require!(
            resolution.tournament_gain == two_stakes,
            CoordinationError::ArithmeticOverflow
        );
        Ok(resolution)
    }
}

/// Computes payoffs for a different-team (heterogeneous) matchup.
///
/// Correct guess = GUESS_DIFF_TEAM (1), since the players are on different teams.
///
/// Winner determination:
///   - If exactly one player is wrong: the wrong player loses; the correct player wins
///   - If both correct or both wrong: first committer wins (recorded on-chain)
///
/// Payoffs:
///   Winner return: stake * 19 / 10  (1.9× stake — net gain of 0.9× opponent's stake)
///   Loser return: 0
///   Tournament gain: 2 * stake - winner_return  (= stake / 10)
///
/// Invariant: winner_return + 0 + tournament_gain == 2 * stake_lamports
pub fn resolve_heterogeneous(
    p1_guess: u8,
    p2_guess: u8,
    stake_lamports: u64,
    first_committer: u8,
) -> Result<Resolution> {
    // Invariant: stake_lamports must be nonzero
    require!(stake_lamports > 0, CoordinationError::ArithmeticOverflow);
    // Invariant: first_committer must be 1 (p1) or 2 (p2)
    require!(
        first_committer == 1 || first_committer == 2,
        CoordinationError::InvalidGameState
    );

    let two_stakes = stake_lamports
        .checked_mul(2)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    // Winner return = 1.9× stake; tournament_gain = remainder to avoid rounding loss
    let winner_return = stake_lamports
        .checked_mul(19)
        .and_then(|v| v.checked_div(10))
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let tournament_gain = two_stakes
        .checked_sub(winner_return)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    // Assert invariant before branching
    let total = winner_return
        .checked_add(tournament_gain)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    require!(total == two_stakes, CoordinationError::ArithmeticOverflow);

    let p1_correct = p1_guess == crate::state::GUESS_DIFF_TEAM;
    let p2_correct = p2_guess == crate::state::GUESS_DIFF_TEAM;

    // If exactly one is wrong, the correct player wins regardless of commit order.
    // Otherwise (both correct or both wrong), first committer wins.
    let p1_wins = if p1_correct == p2_correct {
        first_committer == 1
    } else {
        p1_correct
    };

    if p1_wins {
        Ok(Resolution {
            p1_return: winner_return,
            p2_return: 0,
            tournament_gain,
        })
    } else {
        Ok(Resolution {
            p1_return: 0,
            p2_return: winner_return,
            tournament_gain,
        })
    }
}

/// Routes to the appropriate payoff function based on matchup_type.
///
/// matchup_type == 0: same-team (homogenous)
/// matchup_type == 1: different-team (heterogeneous)
pub fn resolve_game(
    matchup_type: u8,
    p1_guess: u8,
    p2_guess: u8,
    stake_lamports: u64,
    first_committer: u8,
) -> Result<Resolution> {
    match matchup_type {
        0 => resolve_homogenous(p1_guess, p2_guess, stake_lamports),
        1 => resolve_heterogeneous(p1_guess, p2_guess, stake_lamports, first_committer),
        _ => Err(error!(CoordinationError::InvalidGameState)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{GUESS_DIFF_TEAM, GUESS_SAME_TEAM};

    fn assert_invariant(r: &Resolution, stake: u64) {
        let total = r
            .p1_return
            .checked_add(r.p2_return)
            .unwrap()
            .checked_add(r.tournament_gain)
            .unwrap();
        assert_eq!(
            total,
            stake.checked_mul(2).unwrap(),
            "lamports must be conserved"
        );
    }

    // ---------------------------------------------------------------------------
    // resolve_homogenous
    // ---------------------------------------------------------------------------

    #[test]
    fn both_correct_returns_90_percent() {
        let stake = 1_000_000; // 0.001 SOL
        let r = resolve_homogenous(GUESS_SAME_TEAM, GUESS_SAME_TEAM, stake).unwrap();
        assert_eq!(r.p1_return, 900_000);
        assert_eq!(r.p2_return, 900_000);
        assert_eq!(r.tournament_gain, 200_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn p1_wrong_both_forfeit() {
        let stake = 1_000_000;
        let r = resolve_homogenous(GUESS_DIFF_TEAM, GUESS_SAME_TEAM, stake).unwrap();
        assert_eq!(r.p1_return, 0);
        assert_eq!(r.p2_return, 0);
        assert_eq!(r.tournament_gain, 2_000_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn p2_wrong_both_forfeit() {
        let stake = 1_000_000;
        let r = resolve_homogenous(GUESS_SAME_TEAM, GUESS_DIFF_TEAM, stake).unwrap();
        assert_eq!(r.p1_return, 0);
        assert_eq!(r.p2_return, 0);
        assert_eq!(r.tournament_gain, 2_000_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn both_wrong_both_forfeit() {
        let stake = 1_000_000;
        let r = resolve_homogenous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, stake).unwrap();
        assert_eq!(r.p1_return, 0);
        assert_eq!(r.p2_return, 0);
        assert_eq!(r.tournament_gain, 2_000_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn homogenous_lamports_conserved_various_stakes() {
        for stake in [100, 999, 1_000_000, 10_000_000_000u64] {
            let r = resolve_homogenous(GUESS_SAME_TEAM, GUESS_SAME_TEAM, stake).unwrap();
            assert_invariant(&r, stake);
            let r2 = resolve_homogenous(GUESS_DIFF_TEAM, GUESS_SAME_TEAM, stake).unwrap();
            assert_invariant(&r2, stake);
        }
    }

    #[test]
    fn homogenous_zero_stake_errors() {
        assert!(resolve_homogenous(GUESS_SAME_TEAM, GUESS_SAME_TEAM, 0).is_err());
    }

    // ---------------------------------------------------------------------------
    // resolve_heterogeneous
    // ---------------------------------------------------------------------------

    #[test]
    fn hetero_both_correct_p1_commits_first_p1_wins() {
        let stake = 1_000_000;
        // Both correct, p1 committed first → p1 wins
        let r = resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, stake, 1).unwrap();
        assert_eq!(r.p1_return, 1_900_000); // 1.9× stake
        assert_eq!(r.p2_return, 0);
        assert_eq!(r.tournament_gain, 100_000); // stake/10
        assert_invariant(&r, stake);
    }

    #[test]
    fn hetero_both_correct_p2_commits_first_p2_wins() {
        let stake = 1_000_000;
        // Both correct, p2 committed first → p2 wins
        let r = resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, stake, 2).unwrap();
        assert_eq!(r.p1_return, 0);
        assert_eq!(r.p2_return, 1_900_000);
        assert_eq!(r.tournament_gain, 100_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn hetero_p1_wrong_p2_correct_p2_wins_regardless_of_commit_order() {
        let stake = 1_000_000;
        // P1 wrong, p2 correct → p2 wins regardless of who committed first
        let r1 = resolve_heterogeneous(GUESS_SAME_TEAM, GUESS_DIFF_TEAM, stake, 1).unwrap();
        assert_eq!(r1.p1_return, 0);
        assert_eq!(r1.p2_return, 1_900_000);
        assert_invariant(&r1, stake);

        let r2 = resolve_heterogeneous(GUESS_SAME_TEAM, GUESS_DIFF_TEAM, stake, 2).unwrap();
        assert_eq!(r2.p1_return, 0);
        assert_eq!(r2.p2_return, 1_900_000);
        assert_invariant(&r2, stake);
    }

    #[test]
    fn hetero_p2_wrong_p1_correct_p1_wins_regardless_of_commit_order() {
        let stake = 1_000_000;
        // P2 wrong, p1 correct → p1 wins regardless of who committed first
        let r1 = resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_SAME_TEAM, stake, 1).unwrap();
        assert_eq!(r1.p1_return, 1_900_000);
        assert_eq!(r1.p2_return, 0);
        assert_invariant(&r1, stake);

        let r2 = resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_SAME_TEAM, stake, 2).unwrap();
        assert_eq!(r2.p1_return, 1_900_000);
        assert_eq!(r2.p2_return, 0);
        assert_invariant(&r2, stake);
    }

    #[test]
    fn hetero_both_wrong_p1_commits_first_p1_wins() {
        let stake = 1_000_000;
        // Both wrong, p1 committed first → p1 wins (first inaccurate wins)
        let r = resolve_heterogeneous(GUESS_SAME_TEAM, GUESS_SAME_TEAM, stake, 1).unwrap();
        assert_eq!(r.p1_return, 1_900_000);
        assert_eq!(r.p2_return, 0);
        assert_invariant(&r, stake);
    }

    #[test]
    fn hetero_both_wrong_p2_commits_first_p2_wins() {
        let stake = 1_000_000;
        // Both wrong, p2 committed first → p2 wins
        let r = resolve_heterogeneous(GUESS_SAME_TEAM, GUESS_SAME_TEAM, stake, 2).unwrap();
        assert_eq!(r.p1_return, 0);
        assert_eq!(r.p2_return, 1_900_000);
        assert_invariant(&r, stake);
    }

    #[test]
    fn hetero_lamports_conserved_various_stakes() {
        for stake in [100, 999, 1_000_000, 10_000_000_000u64] {
            for first_committer in [1u8, 2u8] {
                for (g1, g2) in [
                    (GUESS_DIFF_TEAM, GUESS_DIFF_TEAM),
                    (GUESS_SAME_TEAM, GUESS_SAME_TEAM),
                    (GUESS_DIFF_TEAM, GUESS_SAME_TEAM),
                    (GUESS_SAME_TEAM, GUESS_DIFF_TEAM),
                ] {
                    let r = resolve_heterogeneous(g1, g2, stake, first_committer).unwrap();
                    assert_invariant(&r, stake);
                }
            }
        }
    }

    #[test]
    fn hetero_zero_stake_errors() {
        assert!(resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, 0, 1).is_err());
    }

    #[test]
    fn hetero_invalid_first_committer_errors() {
        assert!(resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, 1_000_000, 0).is_err());
        assert!(resolve_heterogeneous(GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, 1_000_000, 3).is_err());
    }

    // ---------------------------------------------------------------------------
    // resolve_game router
    // ---------------------------------------------------------------------------

    #[test]
    fn resolve_game_routes_homogenous() {
        let stake = 1_000_000;
        let r = resolve_game(0, GUESS_SAME_TEAM, GUESS_SAME_TEAM, stake, 1).unwrap();
        assert_eq!(r.p1_return, 900_000);
        assert_eq!(r.p2_return, 900_000);
    }

    #[test]
    fn resolve_game_routes_heterogeneous() {
        let stake = 1_000_000;
        let r = resolve_game(1, GUESS_DIFF_TEAM, GUESS_DIFF_TEAM, stake, 1).unwrap();
        assert_eq!(r.p1_return, 1_900_000);
        assert_eq!(r.p2_return, 0);
    }

    #[test]
    fn resolve_game_invalid_matchup_type_errors() {
        assert!(resolve_game(2, GUESS_SAME_TEAM, GUESS_SAME_TEAM, 1_000_000, 1).is_err());
    }
}
