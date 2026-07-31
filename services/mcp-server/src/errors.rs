#[derive(Debug, thiserror::Error)]
pub enum McpServiceError {
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
