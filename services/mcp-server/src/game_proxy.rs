use crate::errors::McpServiceError;

// Re-export shared types used by tools.rs
pub use game_api_client::{LeaderboardResponse, QueueJoinResponse};

/// Thin adapter around the shared `GameApiClient` that maps errors to `McpServiceError`.
///
/// The MCP server only uses a subset of the shared client's methods (auth_challenge,
/// join_queue, get_leaderboard). This wrapper provides the same method signatures
/// that tools.rs expects while delegating to the shared crate.
pub struct GameApiProxy {
    client: game_api_client::GameApiClient,
}

impl GameApiProxy {
    pub fn new(base_url: String) -> anyhow::Result<Self> {
        let client = game_api_client::GameApiClient::new(&base_url)
            .map_err(|e| anyhow::anyhow!("game-api client build failed: {e}"))?;

        Ok(Self { client })
    }

    /// Request an auth challenge nonce for a wallet.
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

    /// Get leaderboard for a tournament.
    pub async fn get_leaderboard(
        &self,
        tournament_id: u64,
        limit: Option<u32>,
    ) -> Result<LeaderboardResponse, McpServiceError> {
        self.client
            .get_leaderboard(tournament_id, limit)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaderboard_response_serialization() {
        let resp = LeaderboardResponse {
            entries: vec![game_api_client::LeaderboardEntry {
                wallet: "ABC123".to_string(),
                wins: 10,
                total_games: 20,
                score: 500,
            }],
            tournament_id: 1,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: LeaderboardResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].wins, 10);
    }
}
