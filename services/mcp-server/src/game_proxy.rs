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
    /// DEPRECATED — leaks the public wallet; prefer `xqueue_status_by_handle`.
    pub async fn xqueue_status(
        &self,
        wallet: &str,
    ) -> Result<game_api_client::XQueueResponse, McpServiceError> {
        self.client
            .xqueue_status(wallet)
            .await
            .map_err(map_game_api_error)
    }

    /// Poll for a cross-chain match by the secret handle minted at `/join`.
    pub async fn xqueue_status_by_handle(
        &self,
        handle: &str,
    ) -> Result<game_api_client::XQueueResponse, McpServiceError> {
        self.client
            .xqueue_status_by_handle(handle)
            .await
            .map_err(map_game_api_error)
    }

    /// Join the same-chain EVM (EVM-vs-EVM) queue (proxies game-api's internal
    /// endpoint). No session key — same-chain gameplay is on-chain.
    pub async fn evmgame_join(
        &self,
        wallet: &str,
        chain: &str,
        tournament_id: u64,
    ) -> Result<game_api_client::XQueueResponse, McpServiceError> {
        let request = game_api_client::EvmGameJoinRequest {
            wallet,
            chain,
            tournament_id,
            // The MCP surface serves external AI agents, so the same-chain EVM
            // join is is_ai=true — mirroring the Solana `join_queue_after_stake`
            // path. game-api derives a heterogeneous matchup when this meets a
            // human browser (is_ai=false) and homogeneous vs another agent.
            is_ai: true,
            is_internal: false,
        };
        self.client
            .evmgame_join(&request)
            .await
            .map_err(map_game_api_error)
    }

    /// Poll for a same-chain EVM match by wallet.
    pub async fn evmgame_status(
        &self,
        wallet: &str,
    ) -> Result<game_api_client::XQueueResponse, McpServiceError> {
        self.client
            .evmgame_status(wallet)
            .await
            .map_err(map_game_api_error)
    }

    /// Notify that a player committed on-chain; once both have, the response
    /// carries `r_matchup`.
    pub async fn evmgame_committed(
        &self,
        game_id: &str,
        wallet: &str,
    ) -> Result<serde_json::Value, McpServiceError> {
        self.client
            .evmgame_committed(game_id, wallet)
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

// Flow tests with mocked I/O (backend standard): every proxy fn that the
// game_*/xchain_* tools route through gets a happy-path test asserting the
// exact HTTP shape (method, path, body/query threading) against a wiremock
// game-api, plus error-path coverage for the shared non-2xx mapping. Mirrors
// the OrchestratorProxy wiremock tests in proxy.rs.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn proxy(server: &MockServer) -> GameApiProxy {
        GameApiProxy::new(server.uri()).expect("proxy builds against mock uri")
    }

    #[tokio::test]
    async fn auth_challenge_posts_wallet_and_parses_nonce() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/challenge"))
            .and(body_partial_json(json!({ "wallet": "walletA" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "nonce": "n-123" })))
            .expect(1)
            .mount(&server)
            .await;

        let resp = proxy(&server).auth_challenge("walletA").await.expect("ok");
        assert_eq!(resp.nonce, "n-123");
    }

    #[tokio::test]
    async fn join_queue_sends_bearer_token_and_external_flag() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/queue/join"))
            .and(header("authorization", "Bearer jwt-1"))
            // The proxy serves external agents: is_internal must be false.
            .and(body_partial_json(
                json!({ "tournament_id": 7, "is_ai": false, "is_internal": false }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "matched": true, "session_id": "s-1", "cohort_name": null }),
            ))
            .expect(1)
            .mount(&server)
            .await;

        let resp = proxy(&server)
            .join_queue("jwt-1", 7, false, "test/v1")
            .await
            .expect("ok");
        assert!(resp.matched);
        assert_eq!(resp.session_id.as_deref(), Some("s-1"));
    }

    #[tokio::test]
    async fn xqueue_join_threads_full_request_and_parses_matched() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/internal/xqueue/join"))
            .and(body_partial_json(json!({
                "wallet": "So1Wallet",
                "chain": "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1",
                "session_key": "0x1111111111111111111111111111111111111111",
                "tournament_id": 2,
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({ "status": "matched", "match": { "match_id": "0xabc" } }),
                ),
            )
            .expect(1)
            .mount(&server)
            .await;

        let resp = proxy(&server)
            .xqueue_join(
                "So1Wallet",
                "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1",
                "0x1111111111111111111111111111111111111111",
                2,
            )
            .await
            .expect("ok");
        assert_eq!(resp.status, "matched");
        assert_eq!(resp.match_payload.expect("payload")["match_id"], "0xabc");
    }

    #[tokio::test]
    async fn xqueue_status_queries_by_wallet() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/internal/xqueue/status"))
            .and(query_param("wallet", "So1Wallet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "status": "waiting" })))
            .expect(1)
            .mount(&server)
            .await;

        let resp = proxy(&server).xqueue_status("So1Wallet").await.expect("ok");
        assert_eq!(resp.status, "waiting");
        assert!(resp.match_payload.is_none());
    }

    #[tokio::test]
    async fn xqueue_status_by_handle_sends_handle_not_wallet() {
        // The anonymity fix: polling must go out as ?handle=, and the public
        // wallet must NOT appear in the query string.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/internal/xqueue/status"))
            .and(query_param("handle", "sekret-uuid"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "status": "matched", "match": { "match_id": "0x1" } })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let resp = proxy(&server)
            .xqueue_status_by_handle("sekret-uuid")
            .await
            .expect("ok");
        assert_eq!(resp.status, "matched");
    }

    #[tokio::test]
    async fn evmgame_join_and_status_hit_the_evmgame_endpoints() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/internal/evmgame/join"))
            .and(body_partial_json(json!({
                "wallet": "0xabc",
                "chain": "eip155:8453",
                "tournament_id": 3,
                "is_ai": true,
                "is_internal": false,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "status": "waiting" })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/internal/evmgame/status"))
            .and(query_param("wallet", "0xabc"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "status": "matched", "match": { "game_id": "0xg" } })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let p = proxy(&server);
        let joined = p.evmgame_join("0xabc", "eip155:8453", 3).await.expect("ok");
        assert_eq!(joined.status, "waiting");
        let status = p.evmgame_status("0xabc").await.expect("ok");
        assert_eq!(status.match_payload.expect("payload")["game_id"], "0xg");
    }

    #[tokio::test]
    async fn evmgame_committed_posts_game_and_wallet() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/internal/evmgame/committed"))
            .and(body_partial_json(
                json!({ "game_id": "0xg", "wallet": "0xabc" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "both_committed": true, "r_matchup": "0xr" })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let resp = proxy(&server)
            .evmgame_committed("0xg", "0xabc")
            .await
            .expect("ok");
        assert_eq!(resp["both_committed"], true);
        assert_eq!(resp["r_matchup"], "0xr");
    }

    #[tokio::test]
    async fn build_sol_fund_parses_the_cosigned_tx_bundle() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/internal/xqueue/build-sol-fund"))
            .and(body_partial_json(json!({ "wallet": "So1Wallet" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "unsigned_tx": "dHg=",
                "blockhash": "Bh111",
                "matchmaker_signature": "c2ln",
                "match_id": "0xm",
                "action": "create_xmatch",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let resp = proxy(&server)
            .xqueue_build_sol_fund("So1Wallet")
            .await
            .expect("ok");
        assert_eq!(resp.unsigned_tx, "dHg=");
        assert_eq!(resp.matchmaker_signature, "c2ln");
        assert_eq!(resp.action, "create_xmatch");
    }

    #[tokio::test]
    async fn sol_lock_outcome_cosign_and_gameplay_pass_through_json() {
        let server = MockServer::start().await;
        for (p, m) in [
            ("/internal/xqueue/build-sol-lock", "POST"),
            ("/internal/xqueue/outcome-cosign", "POST"),
            ("/internal/xqueue/gameplay", "GET"),
        ] {
            Mock::given(method(m))
                .and(path(p))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "endpoint": p })))
                .expect(1)
                .mount(&server)
                .await;
        }

        let p = proxy(&server);
        let lock = p.xqueue_build_sol_lock("w").await.expect("ok");
        assert_eq!(lock["endpoint"], "/internal/xqueue/build-sol-lock");
        let cosign = p.xqueue_outcome_cosign("w").await.expect("ok");
        assert_eq!(cosign["endpoint"], "/internal/xqueue/outcome-cosign");
        let gameplay = p.xqueue_gameplay("w").await.expect("ok");
        assert_eq!(gameplay["endpoint"], "/internal/xqueue/gameplay");
    }

    #[tokio::test]
    async fn build_sol_refund_threads_match_id_and_kind() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/internal/xqueue/build-sol-refund"))
            .and(body_partial_json(
                json!({ "wallet": "So1Wallet", "match_id": "0xm", "kind": "timeout" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "action": "refund_xmatch_timeout" })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let resp = proxy(&server)
            .xqueue_build_sol_refund("So1Wallet", "0xm", "timeout")
            .await
            .expect("ok");
        assert_eq!(resp["action"], "refund_xmatch_timeout");
    }

    #[tokio::test]
    async fn commit_sign_reveal_thread_gameplay_bodies() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/internal/xqueue/commit"))
            .and(body_partial_json(json!({ "wallet": "w", "commit": "0xc" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "status": "ok" })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/internal/xqueue/sign"))
            .and(body_partial_json(
                json!({ "wallet": "w", "step": 4, "signature": "0xs" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "relayed": true })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/internal/xqueue/reveal"))
            .and(body_partial_json(
                json!({ "wallet": "w", "preimage": "0xp" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "revealed": true })))
            .expect(1)
            .mount(&server)
            .await;

        let p = proxy(&server);
        assert_eq!(
            p.xqueue_commit("w", "0xc").await.expect("ok")["status"],
            "ok"
        );
        assert_eq!(
            p.xqueue_sign("w", 4, "0xs").await.expect("ok")["relayed"],
            true
        );
        assert_eq!(
            p.xqueue_reveal("w", "0xp").await.expect("ok")["revealed"],
            true
        );
    }

    // Error path: a non-2xx from game-api must surface as GameApiError (with
    // the body preserved for diagnostics), never a silent default.
    #[tokio::test]
    async fn non_2xx_maps_to_game_api_error_with_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/internal/xqueue/join"))
            .respond_with(ResponseTemplate::new(400).set_body_string(r#"{"error":"bad_request"}"#))
            .expect(1)
            .mount(&server)
            .await;

        let err = proxy(&server)
            .xqueue_join("w", "chain", "0xkey", 1)
            .await
            .expect_err("must fail");
        match err {
            McpServiceError::GameApiError(msg) => {
                assert!(msg.contains("400"), "carries the status: {msg}");
                assert!(msg.contains("bad_request"), "carries the body: {msg}");
            }
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    // Error path: connection-level failure (game-api down) also maps to
    // GameApiError rather than panicking or swallowing.
    #[tokio::test]
    async fn connection_failure_maps_to_game_api_error() {
        // Bind-then-drop a server so the port is dead.
        let server = MockServer::start().await;
        let uri = server.uri();
        drop(server);

        let p = GameApiProxy::new(uri).expect("builds");
        let err = p.xqueue_status("w").await.expect_err("must fail");
        assert!(matches!(err, McpServiceError::GameApiError(_)));
    }
}
