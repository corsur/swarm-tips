use anchor_lang::prelude::*;

pub const COMMIT_TIMEOUT_SLOTS: u64 = 7_200;
pub const REVEAL_TIMEOUT_SLOTS: u64 = 14_400;
pub const FIXED_STAKE_LAMPORTS: u64 = 10_000_000; // 0.01 SOL

pub const GUESS_SAME_TEAM: u8 = 0;
pub const GUESS_DIFF_TEAM: u8 = 1;
pub const GUESS_UNREVEALED: u8 = 255;

#[account]
pub struct GameCounter {
    pub count: u64,
    pub bump: u8,
}

impl GameCounter {
    pub const SPACE: usize = 8 + 8 + 1;
}

#[account]
pub struct Game {
    pub game_id: u64,
    pub tournament_id: u64,
    pub player_one: Pubkey,
    pub player_two: Pubkey,
    pub state: GameState,
    pub stake_lamports: u64,
    pub p1_commit: [u8; 32],
    pub p2_commit: [u8; 32],
    pub p1_guess: u8,
    pub p2_guess: u8,
    pub first_committer: u8,
    pub p1_commit_slot: u64,
    pub p2_commit_slot: u64,
    pub commit_timeout_slots: u64,
    pub created_at: i64,
    pub resolved_at: i64,
    /// 0 = same team (homogenous), 1 = different teams (heterogeneous).
    pub matchup_type: u8,
    pub bump: u8,
}

impl Game {
    // discriminator + all fields
    pub const SPACE: usize = 8
        + 8   // game_id
        + 8   // tournament_id
        + 32  // player_one
        + 32  // player_two
        + 1   // state (enum tag)
        + 8   // stake_lamports
        + 32  // p1_commit
        + 32  // p2_commit
        + 1   // p1_guess
        + 1   // p2_guess
        + 1   // first_committer
        + 8   // p1_commit_slot
        + 8   // p2_commit_slot
        + 8   // commit_timeout_slots
        + 8   // created_at
        + 8   // resolved_at
        + 1   // matchup_type
        + 1; // bump
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum GameState {
    Pending,
    Active,
    Committing,
    Revealing,
    Resolved,
}

// Compile-time invariant: reveal window must be longer than commit window.
const _: () = assert!(REVEAL_TIMEOUT_SLOTS > COMMIT_TIMEOUT_SLOTS);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_constants_are_distinct() {
        assert_ne!(GUESS_SAME_TEAM, GUESS_DIFF_TEAM);
        assert_ne!(GUESS_SAME_TEAM, GUESS_UNREVEALED);
        assert_ne!(GUESS_DIFF_TEAM, GUESS_UNREVEALED);
    }

    #[test]
    fn game_state_equality() {
        assert_eq!(GameState::Pending, GameState::Pending);
        assert_ne!(GameState::Pending, GameState::Active);
        assert_ne!(GameState::Committing, GameState::Revealing);
    }

    #[test]
    fn timeout_slots_ordering() {
        // Reveal timeout must be longer than commit timeout — verified at
        // compile time by the const assertion in the parent module.
        assert_eq!(REVEAL_TIMEOUT_SLOTS, 14_400);
        assert_eq!(COMMIT_TIMEOUT_SLOTS, 7_200);
    }
}
