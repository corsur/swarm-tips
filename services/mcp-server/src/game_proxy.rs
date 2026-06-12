use crate::errors::McpServiceError;

// Re-export shared types used by tools.rs
pub use game_api_client::QueueJoinResponse;

/// Thin adapter around the shared `GameApiClient` that maps errors to
/// `McpServiceError`. The leaderboard is read directly from on-chain
/// PlayerProfile PDAs (see `solana_reads::read_all_player_profiles_for_tournament`),
/// not through this proxy.
pub struct GameApiProxy {
    client: game_api_client::GameApiClient,
}

impl GameApiProxy {
    pub fn new(base_url: String) -> anyhow::Result<Self> {
        let client = game_api_client::GameApiClient::new(&base_url)
            .map_err(|e| anyhow::anyhow!("game-api client build failed: {e}"))?;

        Ok(Self { client })
    }

    /// Request an auth challenge nonce for a wallet. Currently unused at the
    /// MCP tool layer; kept as a reusable adapter for any future pattern that
    /// needs the challenge/sign/JWT flow.
    #[allow(dead_code)]
    pub async fn auth_challenge(
        &self,
        wallet: &str,
    ) -> Result<game_api_client::ChallengeResponse, McpServiceError> {
        self.client
            .request_challenge(wallet)
            .await
            .map_err(map_game_api_error)
    }

    /// Join the matchmaking queue.
    #[allow(dead_code)]
    pub async fn join_queue(
        &self,
        token: &str,
        tournament_id: u64,
        is_ai: bool,
        agent_version: &str,
    ) -> Result<QueueJoinResponse, McpServiceError> {
        let request = game_api_client::QueueJoinRequest {
            tournament_id,
            is_ai,
            agent_version,
            is_internal: false, // proxy serves external agents
        };

        self.client
            .join_queue(token, &request)
            .await
            .map_err(map_game_api_error)
    }

    /// Join the cross-chain queue (proxies game-api's internal endpoint).
    pub async fn xqueue_join(
        &self,
        wallet: &str,
        chain: &str,
        session_key: &str,
        tournament_id: u64,
    ) -> Result<game_api_client::XQueueResponse, McpServiceError> {
        let request = game_api_client::XQueueJoinRequest {
            wallet,
            chain,
            session_key,
            tournament_id,
        };
        self.client
            .xqueue_join(&request)
            .await
            .map_err(map_game_api_error)
    }

    /// Poll for a cross-chain match by chain-native wallet.
    pub async fn xqueue_status(
        &self,
        wallet: &str,
    ) -> Result<game_api_client::XQueueResponse, McpServiceError> {
        self.client
            .xqueue_status(wallet)
            .await
            .map_err(map_game_api_error)
    }
}

/// Map shared crate errors to MCP server errors with structured logging.
fn map_game_api_error(err: game_api_client::GameApiError) -> McpServiceError {
    tracing::error!(
        service = "coordination-mcp-server",
        error = %err,
        "game-api request failed"
    );
    McpServiceError::GameApiError(err.to_string())
}
