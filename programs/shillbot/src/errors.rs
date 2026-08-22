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

    // Do NOT restate the cap here. The bound is GlobalState.max_concurrent_claims,
    // which is authority-tunable via update_params; this message used to hardcode
    // "5" while the live value was 50, so anyone reading the error concluded the
    // agent held 5 claims and mis-sized the recovery.
    #[msg("Agent is at GlobalState.max_concurrent_claims active claims")]
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

    #[msg("Expire called on Submitted task before verification timeout reached")]
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

    #[msg("SlotHashes sysvar data is malformed (less than 24 bytes)")]
    MalformedSlotHashes,

    #[msg(
        "Instruction parameter is outside its allowed range or is zero where positive is required"
    )]
    InvalidParameter,

    #[msg("Pubkey parameter must not be the zero pubkey")]
    ZeroPubkey,

    #[msg("Operation requires the agent to have at least one active claim")]
    NoActiveClaim,

    #[msg("AgentState cannot be closed while it has active claims")]
    AgentStateHasActiveClaims,

    #[msg("Payout route is already set and cannot be changed (one-shot, set early and locked)")]
    PayoutRouteLocked,

    #[msg("Payout route target is invalid (must not be zero, the task PDA, or the client)")]
    InvalidPayoutTarget,

    #[msg("Unknown verification kind (append-only: 0 = OracleMetrics, 1 = DeterministicAttested)")]
    InvalidVerificationKind,

    #[msg("Task's verification kind does not admit this verify instruction")]
    VerificationKindMismatch,

    #[msg("Attested score must be exactly 0 or MAX_SCORE (binary verification)")]
    AttestedScoreNotBinary,

    #[msg("quality_threshold == MAX_SCORE would pay 0 for a passing binary score — fix params")]
    QualityThresholdTooHighForBinary,

    #[msg("Attester must be arms-length from the task agent")]
    ArmsLengthViolation,

    /// DEPRECATED (no longer thrown): the kind-1 self-claim guard was removed —
    /// it only blocked same-wallet credential wash, which a two-wallet split
    /// bypasses. Retained to preserve Anchor error numbering.
    #[msg("Deterministic tasks reject self-claims (agent must not be the client)")]
    SelfClaimForbidden,

    #[msg("Dispute default-resolution is disabled (dispute_resolution_window_seconds == 0)")]
    DisputeWindowDisabled,

    #[msg("Dispute resolution window still open — authority resolve_challenge only")]
    DisputeWindowStillOpen,

    /// Account is not owned by this program. Previously this guard
    /// reported InvalidTaskState, which describes an unrelated condition.
    #[msg("Account is not owned by this program")]
    InvalidAccountOwner,
}
