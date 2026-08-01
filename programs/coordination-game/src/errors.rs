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

    // Session
    #[msg("Session has expired")]
    SessionExpired,
    #[msg("Session authority does not match the player")]
    SessionPlayerMismatch,
    #[msg("Session signer does not match the session key")]
    SessionSignerMismatch,

    // GlobalConfig
    #[msg("Caller is not the GlobalConfig authority")]
    NotAuthority,
    #[msg("Caller is not the authorized matchmaker")]
    NotMatchmaker,
    #[msg("Treasury split basis points out of bounds [2000, 8000]")]
    InvalidTreasurySplitBps,

    // Merkle
    #[msg("Merkle proof verification failed")]
    InvalidMerkleProof,
    #[msg("Merkle proof exceeds maximum depth (20 levels)")]
    MerkleProofTooLong,

    // Transfer
    #[msg("Source account has insufficient lamports for transfer")]
    InsufficientLamports,

    // Sweep
    #[msg("Unclaimed grace period has not elapsed (T+90 days from end_time)")]
    UnclaimedGracePeriodNotElapsed,
    #[msg("Destination tournament is invalid (same as source, finalized, or outside its active window)")]
    DestTournamentInvalid,

    // Reveal
    #[msg("r_matchup must not be passed once the matchup type is already revealed in the Game account")]
    RMatchupMismatch,

    // Cross-chain
    #[msg("Cross-chain match is in the wrong status for this instruction")]
    XInvalidStatus,
    #[msg("Certificate terms do not match the recorded escrow state")]
    XCertMismatch,
    #[msg("Certificate signature did not recover the expected signer")]
    XBadSignature,
    #[msg("Rate quote is stale relative to the tranche lock")]
    XStaleQuote,
    #[msg("Deadline has not been reached yet")]
    XDeadlineNotReached,
    #[msg("Deadline has already passed")]
    XDeadlinePassed,
    #[msg("Payout pool has insufficient free balance")]
    XPoolInsufficient,
    #[msg("Tranche exceeds the configured maximum")]
    XTrancheTooLarge,
    #[msg("Cross-chain configuration is invalid")]
    XBadConfig,
    #[msg("Outcome kind is not valid for this settlement path")]
    XBadOutcome,

    /// Treasury must not be the zero pubkey. Previously this rejection
    /// reused NotAuthority, which describes an unrelated condition.
    #[msg("Treasury must not be the zero pubkey")]
    InvalidTreasury,
}
