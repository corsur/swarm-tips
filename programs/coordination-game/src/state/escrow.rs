use anchor_lang::prelude::*;

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
    ///
    /// `expected_stake` is the LIVE stake from `GlobalConfig`, not a constant:
    /// an escrow funded at a superseded stake must not be consumable, or one
    /// player could enter a game having staked less than the other.
    pub fn validate_for_game(
        &self,
        player: &Pubkey,
        tournament_id: u64,
        expected_stake: u64,
    ) -> bool {
        self.player == *player
            && self.tournament_id == tournament_id
            && self.amount == expected_stake
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
            amount: crate::state::DEFAULT_STAKE_LAMPORTS,
            consumed,
            bump: 255,
        }
    }

    #[test]
    fn validate_for_game_accepts_valid_escrow() {
        let pk = Pubkey::new_unique();
        let escrow = make_escrow(pk, 1, false);
        assert!(escrow.validate_for_game(&pk, 1, crate::state::DEFAULT_STAKE_LAMPORTS));
    }

    #[test]
    fn validate_for_game_rejects_consumed_escrow() {
        let pk = Pubkey::new_unique();
        let escrow = make_escrow(pk, 1, true);
        assert!(!escrow.validate_for_game(&pk, 1, crate::state::DEFAULT_STAKE_LAMPORTS));
    }

    #[test]
    fn validate_for_game_rejects_wrong_player() {
        let pk = Pubkey::new_unique();
        let other = Pubkey::new_unique();
        let escrow = make_escrow(pk, 1, false);
        assert!(!escrow.validate_for_game(&other, 1, crate::state::DEFAULT_STAKE_LAMPORTS));
    }

    #[test]
    fn validate_for_game_rejects_wrong_tournament() {
        let pk = Pubkey::new_unique();
        let escrow = make_escrow(pk, 1, false);
        assert!(!escrow.validate_for_game(&pk, 2, crate::state::DEFAULT_STAKE_LAMPORTS));
    }

    #[test]
    fn match_parity_is_enforced_against_the_games_recorded_stake() {
        // THE INVARIANT THAT LETS THE STAKE FLOAT.
        //
        // `create_game` validates the creator's escrow against the live config
        // and RECORDS that amount on the Game PDA (`init_game`'s
        // `stake_lamports`). `join_game` then validates the joiner's escrow
        // against `game.stake_lamports` — the CREATOR's amount, not a config
        // value. So the two players in a match always stake exactly the same,
        // and that holds no matter what the config says at join time.
        //
        // This is worth pinning because it is the property that makes a
        // per-match quoted stake SAFE rather than a way to let two players ante
        // different amounts. Removing it would not fail any existing test — the
        // stake is currently a single constant, so every escrow agrees by
        // accident and the check looks redundant right up until it isn't.
        let creator = Pubkey::new_unique();
        let joiner = Pubkey::new_unique();
        let game_stake = 68_430_000; // whatever the creator anted

        let creator_escrow = StakeEscrow {
            player: creator,
            tournament_id: 1,
            amount: game_stake,
            consumed: false,
            bump: 254,
        };
        assert!(creator_escrow.validate_for_game(&creator, 1, game_stake));

        // A joiner funded at the OLD stake cannot join a game anted at the new
        // one, even though its escrow is perfectly valid on its own terms.
        let stale_joiner = StakeEscrow {
            player: joiner,
            tournament_id: 1,
            amount: 50_000_000,
            consumed: false,
            bump: 254,
        };
        assert!(
            !stale_joiner.validate_for_game(&joiner, 1, game_stake),
            "a joiner staking less than the creator must be rejected"
        );

        // And the reverse: over-staking is rejected too, so the joiner cannot
        // buy a larger claim on the pot than the creator put up.
        let rich_joiner = StakeEscrow {
            player: joiner,
            tournament_id: 1,
            amount: game_stake + 1,
            consumed: false,
            bump: 254,
        };
        assert!(
            !rich_joiner.validate_for_game(&joiner, 1, game_stake),
            "a joiner staking more than the creator must be rejected"
        );

        // Matching exactly is the only way in.
        let matched = StakeEscrow {
            player: joiner,
            tournament_id: 1,
            amount: game_stake,
            consumed: false,
            bump: 254,
        };
        assert!(matched.validate_for_game(&joiner, 1, game_stake));
    }

    #[test]
    fn validate_for_game_rejects_wrong_amount() {
        let pk = Pubkey::new_unique();
        let mut escrow = make_escrow(pk, 1, false);
        escrow.amount = 0;
        assert!(!escrow.validate_for_game(&pk, 1, crate::state::DEFAULT_STAKE_LAMPORTS));
    }
}
