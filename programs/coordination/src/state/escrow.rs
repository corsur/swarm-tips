use anchor_lang::prelude::*;

use super::FIXED_STAKE_LAMPORTS;

/// Per-player escrow that holds staked lamports while the player is in the
/// matchmaking queue. Created by `deposit_stake`, consumed by `create_game`
/// or `join_game`, refunded by `withdraw_stake`.
///
/// PDA seeds: `["escrow", tournament_id, player]`
#[account]
pub struct StakeEscrow {
    pub player: Pubkey,
    pub tournament_id: u64,
    pub amount: u64,
    /// True once the escrow has been consumed by a create_game or join_game
    /// instruction. Prevents double-spend if the same escrow PDA is reused.
    pub consumed: bool,
    pub bump: u8,
}

impl StakeEscrow {
    // discriminator + all fields
    pub const SPACE: usize = 8
        + 32  // player
        + 8   // tournament_id
        + 8   // amount
        + 1   // consumed
        + 1; // bump

    /// Validate that the escrow is ready to be consumed by a game instruction.
    pub fn validate_for_game(&self, player: &Pubkey, tournament_id: u64) -> bool {
        self.player == *player
            && self.tournament_id == tournament_id
            && self.amount == FIXED_STAKE_LAMPORTS
            && !self.consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_escrow(player: Pubkey, tournament_id: u64, consumed: bool) -> StakeEscrow {
        StakeEscrow {
            player,
            tournament_id,
            amount: FIXED_STAKE_LAMPORTS,
            consumed,
            bump: 255,
        }
    }

    #[test]
    fn validate_for_game_accepts_valid_escrow() {
        let pk = Pubkey::new_unique();
        let escrow = make_escrow(pk, 1, false);
        assert!(escrow.validate_for_game(&pk, 1));
    }

    #[test]
    fn validate_for_game_rejects_consumed_escrow() {
        let pk = Pubkey::new_unique();
        let escrow = make_escrow(pk, 1, true);
        assert!(!escrow.validate_for_game(&pk, 1));
    }

    #[test]
    fn validate_for_game_rejects_wrong_player() {
        let pk = Pubkey::new_unique();
        let other = Pubkey::new_unique();
        let escrow = make_escrow(pk, 1, false);
        assert!(!escrow.validate_for_game(&other, 1));
    }

    #[test]
    fn validate_for_game_rejects_wrong_tournament() {
        let pk = Pubkey::new_unique();
        let escrow = make_escrow(pk, 1, false);
        assert!(!escrow.validate_for_game(&pk, 2));
    }

    #[test]
    fn validate_for_game_rejects_wrong_amount() {
        let pk = Pubkey::new_unique();
        let mut escrow = make_escrow(pk, 1, false);
        escrow.amount = 0;
        assert!(!escrow.validate_for_game(&pk, 1));
    }
}
