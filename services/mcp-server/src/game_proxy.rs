use crate::errors::McpServiceError;

// Re-export shared types used by tools.rs
pub use game_api_client::QueueJoinResponse;

/// Thin adapter around the shared `GameApiClient` that maps errors to `McpServiceError`.
///
/// The MCP server only uses a subset of the shared client's methods (auth_challenge,
/// join_queue). This wrapper provides the same method signatures
/// that tools.rs expects while delegating to the shared crate.
/// `get_leaderboard` was removed in 0-FU-3 (2026-05-07): the underlying
/// `/tournaments/{id}/leaderboard` endpoint never existed in game-api;
/// the leaderboard is now read directly from on-chain PlayerProfile
/// PDAs in `solana_reads::read_all_player_profiles_for_tournament`.
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
    /// MCP tool layer (the previous `game_join_queue` tool that consumed it
    /// was retired 2026-04-08), but kept on the proxy as a reusable adapter
    /// for any future pattern that needs the challenge/sign/JWT flow.
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

    // get_leaderboard was removed 2026-05-07 — see struct doc.
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

// Test for the removed get_leaderboard() proxy was deleted in 0-FU-3
// (2026-05-07) along with the dead HTTP path. The leaderboard's on-chain
// reader now lives in solana_reads::read_all_player_profiles_for_tournament,
// where it has its own coverage.
