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

/// Emitted by `refund_pending` when an un-joined Pending game is cancelled and
/// P1's stake is refunded (mirrors the EVM `GameCancelled`). Powers a log-based
/// metric / alarm on stuck-Pending recovery.
#[event]
pub struct GameCancelled {
    pub game_id: u64,
    pub player_one: Pubkey,
    pub refund_lamports: u64,
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
    pub treasury_gain: u64,
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
    pub merkle_root: [u8; 32],
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

#[event]
pub struct SessionCreated {
    pub player: Pubkey,
    pub session_key: Pubkey,
    pub expires_at: i64,
}

#[event]
pub struct SessionClosed {
    pub player: Pubkey,
    pub session_key: Pubkey,
}

#[event]
pub struct StakeWithdrawn {
    pub wallet: Pubkey,
    pub tournament_id: u64,
    pub amount: u64,
}

#[event]
pub struct ConfigUpdated {
    pub authority: Pubkey,
    pub treasury_split_bps: u16,
}

#[event]
pub struct UnclaimedSwept {
    pub src_tournament_id: u64,
    pub dest_tournament_id: u64,
    pub amount: u64,
}

#[event]
pub struct XMatchCreated {
    pub match_id: [u8; 32],
    pub player: Pubkey,
    pub stake_lamports: u64,
}

#[event]
pub struct XTrancheLocked {
    pub match_id: [u8; 32],
    pub tranche_lamports: u64,
}

#[event]
pub struct XMatchSettled {
    pub match_id: [u8; 32],
    pub outcome_kind: u8,
    pub to_player: u64,
    pub to_treasury: u64,
}

#[event]
pub struct XClaimOpened {
    pub match_id: [u8; 32],
    pub claimed_outcome: u8,
    pub claim_window_end: i64,
}

#[event]
pub struct XClaimSuperseded {
    pub match_id: [u8; 32],
    pub step_count: u8,
    pub claimed_outcome: u8,
}

#[event]
pub struct XEquivocationProven {
    pub match_id: [u8; 32],
    pub new_best_outcome: u8,
}

#[event]
pub struct XPoolDeposited {
    pub funder: Pubkey,
    pub amount: u64,
}

#[event]
pub struct XPoolWithdrawn {
    pub amount: u64,
}

#[event]
pub struct XMatchRefunded {
    pub match_id: [u8; 32],
    pub to_player: u64,
}

/// Emitted when the authority re-pegs the per-game stake. Carries both values
/// so an indexer can reconstruct the peg history without diffing account state.
#[event]
pub struct StakeConfigured {
    pub previous_lamports: u64,
    pub new_lamports: u64,
    pub authority: Pubkey,
}
