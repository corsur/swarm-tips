use anchor_lang::prelude::*;

#[error_code]
pub enum ShillbotError {
    #[msg("Instruction not valid for current task state")]
    InvalidTaskState,

    #[msg("Caller is not the task's client")]
    NotTaskClient,

    #[msg("Caller is not the task's agent")]
    NotTaskAgent,

    #[msg("Caller is not the GlobalState authority")]
    NotAuthority,

    #[msg("Claim or submit attempted after deadline")]
    DeadlineExpired,

    #[msg("Not enough time remaining before deadline to claim")]
    ClaimBufferInsufficient,

    #[msg("Submission too close to deadline")]
    SubmitMarginInsufficient,

    #[msg("Agent has 5 or more active claims")]
    MaxConcurrentClaimsExceeded,

    #[msg("Switchboard account ownership or feed PDA mismatch")]
    InvalidAttestation,

    #[msg("Oracle attestation data outside acceptable staleness window")]
    AttestationStale,

    #[msg("Composite score exceeds MAX_SCORE")]
    ScoreOutOfBounds,

    #[msg("Challenge attempted after window expired")]
    ChallengeWindowClosed,

    #[msg("Finalize attempted before challenge window closes")]
    ChallengeWindowOpen,

    #[msg("Challenge bond below minimum required")]
    InsufficientBond,

    #[msg("Expire called on Submitted task before T+14d verification timeout")]
    VerificationTimeoutNotReached,

    #[msg("Session delegate not authorized for this instruction")]
    InvalidSessionDelegate,

    #[msg("Checked arithmetic overflow or underflow")]
    ArithmeticOverflow,

    #[msg("Computed payment + fee exceeds escrowed lamports")]
    PaymentExceedsEscrow,

    #[msg("Video ID exceeds maximum allowed length")]
    VideoIdTooLong,

    #[msg("Protocol fee basis points outside allowed bounds")]
    ProtocolFeeBoundsExceeded,

    #[msg("Quality threshold outside allowed bounds")]
    QualityThresholdBoundsExceeded,

    #[msg("AgentState account required but not provided in remaining_accounts")]
    MissingAgentState,

    #[msg("remaining_accounts count must be even (task/client pairs)")]
    InvalidAccountPairs,
}
