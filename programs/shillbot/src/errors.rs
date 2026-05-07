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

    #[msg("Content ID exceeds maximum allowed length")]
    ContentIdTooLong,

    #[msg("Protocol fee basis points outside allowed bounds")]
    ProtocolFeeBoundsExceeded,

    #[msg("Quality threshold outside allowed bounds")]
    QualityThresholdBoundsExceeded,

    #[msg("AgentState account required but not provided in remaining_accounts")]
    MissingAgentState,

    #[msg("remaining_accounts count must be even (task/client pairs)")]
    InvalidAccountPairs,

    #[msg("Protocol is paused")]
    ProtocolPaused,

    #[msg("Platform is paused")]
    PlatformPaused,

    #[msg("Invalid platform type")]
    InvalidPlatform,

    #[msg("Oracle authority mismatch")]
    OracleAuthorityMismatch,

    #[msg("Session delegation has expired")]
    SessionExpired,

    #[msg("Timing override value outside allowed bounds")]
    TimingOverrideOutOfBounds,

    #[msg("Identity hash must not be zero")]
    InvalidIdentity,

    #[msg("Verification hash must not be zero")]
    InvalidVerificationHash,

    #[msg("Switchboard feed account does not match the configured feed")]
    SwitchboardFeedMismatch,

    #[msg("Failed to parse Switchboard feed account data")]
    SwitchboardParseError,

    #[msg("Switchboard feed value does not match the provided composite score")]
    SwitchboardScoreMismatch,

    #[msg("Switchboard feed value is negative or zero")]
    SwitchboardInvalidValue,

    #[msg("Switchboard feed not configured in GlobalState")]
    SwitchboardFeedNotConfigured,

    // `EscrowBelowMinimum` (variant 5034) was retired 2026-05-07 alongside
    // the MIN_ESCROW_LAMPORTS gate removal. The variant is preserved as a
    // tombstone here so subsequent variants don't shift their error numbers
    // (Anchor assigns codes by source-order); deserializers seeing 5034 in
    // historical logs should treat it as "MIN_ESCROW gate" — which no longer
    // exists. Future callers cannot return it.
    #[msg("(retired 2026-05-07; see ShillbotError comment) MIN_ESCROW gate removed")]
    EscrowBelowMinimumRetired,

    #[msg("Client exceeded MAX_TASKS_PER_RATE_WINDOW within RATE_LIMIT_WINDOW_SECONDS")]
    RateLimitExceeded,

    #[msg("AgentState account discriminator does not match — refusing to migrate")]
    AgentStateDiscriminatorMismatch,

    #[msg("AgentState account size is unsupported by migrate_agent_state (expected legacy 41 or current 90)")]
    AgentStateUnsupportedSize,

    #[msg("AgentState account agent pubkey does not match the signer")]
    AgentStateAgentMismatch,
}
