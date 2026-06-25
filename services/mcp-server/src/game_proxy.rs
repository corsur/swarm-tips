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

    /// Get the matchmaker-cosigned Solana create_xmatch funding tx.
    pub async fn xqueue_build_sol_fund(
        &self,
        wallet: &str,
    ) -> Result<game_api_client::XSolFundResponse, McpServiceError> {
        self.client
            .xqueue_build_sol_fund(wallet)
            .await
            .map_err(map_game_api_error)
    }

    /// Get the unsigned permissionless `lock_xtranche` tx for the Solana-leg
    /// player. Authorized by the operator's stored match-live signature — no
    /// matchmaker cosign; the player is the cranker/fee payer.
    pub async fn xqueue_build_sol_lock(
        &self,
        wallet: &str,
    ) -> Result<serde_json::Value, McpServiceError> {
        self.client
            .xqueue_build_sol_lock(wallet)
            .await
            .map_err(map_game_api_error)
    }

    /// Get the operator-cosigned cross-chain outcome for settle. The operator
    /// derives the outcome from the stored co-signed checkpoint and signs only
    /// that; the agent adds its session-key signature to assemble settle.
    pub async fn xqueue_outcome_cosign(
        &self,
        wallet: &str,
    ) -> Result<serde_json::Value, McpServiceError> {
        self.client
            .xqueue_outcome_cosign(wallet)
            .await
            .map_err(map_game_api_error)
    }

    /// Get the unsigned Solana refund tx (permissionless) for the player.
    pub async fn xqueue_build_sol_refund(
        &self,
        wallet: &str,
        match_id: &str,
        kind: &str,
    ) -> Result<serde_json::Value, McpServiceError> {
        self.client
            .xqueue_build_sol_refund(wallet, match_id, kind)
            .await
            .map_err(map_game_api_error)
    }

    /// Record the player's guess commit.
    pub async fn xqueue_commit(
        &self,
        wallet: &str,
        commit: &str,
    ) -> Result<serde_json::Value, McpServiceError> {
        self.client
            .xqueue_commit(wallet, commit)
            .await
            .map_err(map_game_api_error)
    }

    /// Submit the player's session-key signature over the canonical checkpoint.
    pub async fn xqueue_sign(
        &self,
        wallet: &str,
        step: u8,
        signature: &str,
    ) -> Result<serde_json::Value, McpServiceError> {
        self.client
            .xqueue_sign(wallet, step, signature)
            .await
            .map_err(map_game_api_error)
    }

    /// Reveal the player's guess preimage.
    pub async fn xqueue_reveal(
        &self,
        wallet: &str,
        preimage: &str,
    ) -> Result<serde_json::Value, McpServiceError> {
        self.client
            .xqueue_reveal(wallet, preimage)
            .await
            .map_err(map_game_api_error)
    }

    /// The player's cross-chain "what to sign next" gameplay view.
    pub async fn xqueue_gameplay(
        &self,
        wallet: &str,
    ) -> Result<serde_json::Value, McpServiceError> {
        self.client
            .xqueue_gameplay(wallet)
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
