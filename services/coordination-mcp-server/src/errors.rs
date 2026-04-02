#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum McpServiceError {
    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("session not found for wallet {0}")]
    SessionNotFound(String),

    #[error("session expired for wallet {0}")]
    SessionExpired(String),

    #[error("rate limit exceeded: {0}")]
    RateLimited(String),

    #[error("orchestrator request failed: {0}")]
    OrchestratorError(String),

    #[error("game api request failed: {0}")]
    GameApiError(String),

    #[error("solana rpc error: {0}")]
    SolanaRpcError(String),

    #[error("transaction construction failed: {0}")]
    TransactionError(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("internal error: {0}")]
    Internal(String),
}
