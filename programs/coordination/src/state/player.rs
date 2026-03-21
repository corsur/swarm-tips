use crate::errors::CoordinationError;
use anchor_lang::prelude::*;

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
        + 1; // bump

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
        let score = wins_sq
            .checked_div(total_games)
            .ok_or_else(|| error!(CoordinationError::ArithmeticOverflow))?;
        // Postcondition: integer division cannot exceed the dividend
        require!(score <= wins_sq, CoordinationError::ArithmeticOverflow);
        Ok(score)
    }

    /// Initializes a freshly created profile. Called from create_game and join_game
    /// via init_if_needed — the account is zeroed on creation, so the condition
    /// guards against re-initializing an existing profile.
    pub fn init_if_new(&mut self, wallet: Pubkey, tournament_id: u64, bump: u8) {
        if self.total_games == 0 && !self.claimed {
            self.wallet = wallet;
            self.tournament_id = tournament_id;
            self.wins = 0;
            self.total_games = 0;
            self.score = 0;
            self.claimed = false;
            self.bump = bump;
        }
    }

    /// Updates wins, total_games, and score after a resolved game.
    /// Shared by reveal_guess and resolve_timeout to keep logic in one place.
    pub fn update_after_game(&mut self, won: bool, tournament_id: u64) -> Result<()> {
        require!(
            self.tournament_id == tournament_id,
            CoordinationError::ProfileTournamentMismatch,
        );
        if won {
            self.wins = self
                .wins
                .checked_add(1)
                .ok_or(CoordinationError::ArithmeticOverflow)?;
        }
        self.total_games = self
            .total_games
            .checked_add(1)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
        self.score = PlayerProfile::compute_score(self.wins, self.total_games)?;
        // Postcondition: wins must never exceed total_games
        require!(
            self.wins <= self.total_games,
            CoordinationError::ArithmeticOverflow,
        );
        Ok(())
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
