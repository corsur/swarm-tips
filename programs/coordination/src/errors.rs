use anchor_lang::prelude::*;

#[error_code]
pub enum CoordinationError {
    // State machine
    #[msg("Invalid game state for this instruction")]
    InvalidGameState,

    // Player validation
    #[msg("Player is not a participant in this game")]
    NotAParticipant,
    #[msg("Player has already committed a guess")]
    AlreadyCommitted,
    #[msg("Player has already revealed a guess")]
    AlreadyRevealed,
    #[msg("Player has already claimed their reward")]
    AlreadyClaimed,
    #[msg("Cannot join your own game")]
    CannotJoinOwnGame,

    // Stake
    #[msg("Stake amount does not match the game's required stake")]
    StakeMismatch,

    // Commit-reveal
    #[msg("Commitment hash mismatch on reveal")]
    CommitmentMismatch,
    #[msg("Revealed guess is not a valid value (must be 0 or 1)")]
    InvalidGuessValue,

    // Timeout
    #[msg("Timeout has not elapsed yet")]
    TimeoutNotElapsed,

    // Tournament
    #[msg("Tournament end_time must be after start_time")]
    InvalidTournamentTimes,
    #[msg("Tournament has not ended yet")]
    TournamentNotEnded,
    #[msg("Tournament must be finalized before rewards can be claimed")]
    TournamentNotFinalized,
    #[msg("Tournament prize pool is empty")]
    EmptyPrizePool,
    #[msg("Game is outside the tournament window")]
    OutsideTournamentWindow,
    #[msg("Player profile does not belong to this tournament")]
    ProfileTournamentMismatch,

    // Eligibility
    #[msg("Player has not played enough games to claim a reward (minimum 5)")]
    BelowMinimumGames,

    // Arithmetic
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,

    // Finalize
    #[msg("Too many accounts passed to finalize_tournament (maximum 30)")]
    TooManyAccounts,

    // Escrow
    #[msg("Escrow has already been consumed by a game")]
    EscrowAlreadyConsumed,
    #[msg("Escrow is not valid for this game (wrong player, tournament, or amount)")]
    EscrowInvalid,
}
