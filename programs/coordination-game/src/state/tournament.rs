use anchor_lang::prelude::*;

#[account]
pub struct Tournament {
    pub tournament_id: u64,
    pub authority: Pubkey,
    pub start_time: i64,
    pub end_time: i64,
    pub prize_lamports: u64,
    pub game_count: u64,
    pub finalized: bool,
    pub prize_snapshot: u64,
    pub merkle_root: [u8; 32],
    pub bump: u8,
}

impl Tournament {
    pub const SPACE: usize = 8
        + 8   // tournament_id
        + 32  // authority
        + 8   // start_time
        + 8   // end_time
        + 8   // prize_lamports
        + 8   // game_count
        + 1   // finalized
        + 8   // prize_snapshot
        + 32  // merkle_root
        + 1; // bump

    pub fn is_active(&self, now: i64) -> bool {
        now >= self.start_time && now <= self.end_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_active_within_window() {
        let t = Tournament {
            tournament_id: 1,
            authority: Pubkey::default(),
            start_time: 100,
            end_time: 200,
            prize_lamports: 0,
            game_count: 0,
            finalized: false,
            prize_snapshot: 0,
            merkle_root: [0u8; 32],
            bump: 255,
        };
        assert!(t.is_active(100));
        assert!(t.is_active(150));
        assert!(t.is_active(200));
        assert!(!t.is_active(99));
        assert!(!t.is_active(201));
    }
}
