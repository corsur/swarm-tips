use anchor_lang::prelude::*;
use crate::errors::CoordinationError;

pub const MIN_GAMES_FOR_PAYOUT: u64 = 5;

#[account]
pub struct PlayerProfile {
    pub wallet: Pubkey,
    pub tournament_id: u64,
    pub wins: u64,
    pub total_games: u64,
    pub score: u64,
    pub claimed: bool,
    pub bump: u8,
}

impl PlayerProfile {
    pub const SPACE: usize = 8
        + 32  // wallet
        + 8   // tournament_id
        + 8   // wins
        + 8   // total_games
        + 8   // score
        + 1   // claimed
        + 1;  // bump

    /// score = wins² / total_games (integer division)
    ///
    /// The quadratic numerator rewards high win *rate*, not raw win count. Integer
    /// division is intentional: players below ~70% win rate score near zero and
    /// are not competitive for the prize pool. This is by design.
    ///
    /// Requires total_games > 0.
    pub fn compute_score(wins: u64, total_games: u64) -> Result<u64> {
        require!(total_games > 0, CoordinationError::ArithmeticOverflow);
        let wins_sq = wins
            .checked_mul(wins)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
        wins_sq
            .checked_div(total_games)
            .ok_or_else(|| error!(CoordinationError::ArithmeticOverflow))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_perfect_record() {
        // 10 wins / 10 games = score 10
        assert_eq!(PlayerProfile::compute_score(10, 10).unwrap(), 10);
    }

    #[test]
    fn score_partial_record() {
        // 3 wins / 10 games = 9 / 10 = 0 (integer division)
        assert_eq!(PlayerProfile::compute_score(3, 10).unwrap(), 0);
        // 9 wins / 10 games = 81 / 10 = 8
        assert_eq!(PlayerProfile::compute_score(9, 10).unwrap(), 8);
    }

    #[test]
    fn score_favors_more_games_over_perfect_small_sample() {
        // 1/1: score = 1*1/1 = 1
        let small = PlayerProfile::compute_score(1, 1).unwrap();
        // 9/10: score = 81/10 = 8
        let large = PlayerProfile::compute_score(9, 10).unwrap();
        assert!(large > small, "9/10 should outscore 1/1");
    }

    #[test]
    fn score_zero_games_errors() {
        assert!(PlayerProfile::compute_score(0, 0).is_err());
    }
}
