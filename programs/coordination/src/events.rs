use anchor_lang::prelude::*;

#[event]
pub struct TournamentCreated {
    pub tournament_id: u64,
    pub start_time: i64,
    pub end_time: i64,
}

#[event]
pub struct GameCreated {
    pub game_id: u64,
    pub tournament_id: u64,
    pub player_one: Pubkey,
    pub stake_lamports: u64,
}

#[event]
pub struct GameStarted {
    pub game_id: u64,
    pub tournament_id: u64,
    pub player_one: Pubkey,
    pub player_two: Pubkey,
}

#[event]
pub struct GuessCommitted {
    pub game_id: u64,
    pub player: Pubkey,
    pub commit_slot: u64,
}

#[event]
pub struct GuessRevealed {
    pub game_id: u64,
    pub player: Pubkey,
}

#[event]
pub struct GameResolved {
    pub game_id: u64,
    pub p1_guess: u8,
    pub p2_guess: u8,
    pub p1_return: u64,
    pub p2_return: u64,
    pub tournament_gain: u64,
}

#[event]
pub struct TimeoutSlash {
    pub game_id: u64,
    pub slashed_player: Pubkey,
    pub slash_amount: u64,
}

#[event]
pub struct TournamentFinalized {
    pub tournament_id: u64,
    pub prize_snapshot: u64,
    pub total_score_snapshot: u64,
}

#[event]
pub struct RewardClaimed {
    pub tournament_id: u64,
    pub player: Pubkey,
    pub amount: u64,
}

#[event]
pub struct StakeDeposited {
    pub player: Pubkey,
    pub tournament_id: u64,
    pub amount: u64,
}
