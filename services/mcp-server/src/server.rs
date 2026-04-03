use crate::auth::ChallengeManager;
use crate::errors::McpServiceError;
use crate::game_proxy::GameApiProxy;
use crate::game_session::GameSessionManager;
use crate::proxy::OrchestratorProxy;
use crate::session::SessionManager;
use crate::solana_tx;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::JsonSchema;
use std::sync::Arc;

/// Shared state accessible to all MCP sessions.
pub struct SharedState {
    pub orchestrator: OrchestratorProxy,
    pub game_api: GameApiProxy,
    pub sessions: Arc<SessionManager>,
    pub solana_rpc_url: String,
    pub program_id: String,
    pub rpc_client: reqwest::Client,
    pub game_sessions: Arc<GameSessionManager>,
    #[allow(dead_code)]
    pub challenge_manager: Arc<ChallengeManager>,
}

/// The Swarm Tips MCP server — unified interface for all DAO verticals.
#[derive(Clone)]
pub struct SwarmTipsMcp {
    tool_router: ToolRouter<Self>,
    state: Arc<SharedState>,
}

// -- Tool parameter structs --

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ListAvailableTasksArgs {
    /// Maximum number of tasks to return (default 20, max 100).
    pub limit: Option<u32>,
    /// Minimum price in lamports to filter tasks (optional).
    pub min_price: Option<u64>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GetTaskDetailsArgs {
    /// The unique task identifier.
    pub task_id: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ClaimTaskArgs {
    /// The unique task identifier (task_counter as u64) to claim.
    pub task_id: String,
    /// The client (task creator) public key, needed for PDA derivation.
    pub client_pubkey: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct SubmitWorkArgs {
    /// The unique task identifier (task_counter as u64).
    pub task_id: String,
    /// The content ID of the completed work (YouTube video ID, tweet ID, etc.).
    pub content_id: String,
    /// The client (task creator) public key, needed for PDA derivation.
    pub client_pubkey: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameJoinQueueArgs {
    /// Tournament ID to join (typically 1 for the current tournament).
    pub tournament_id: u64,
    /// Set to true if you are an AI agent (required for data integrity).
    pub is_ai: bool,
    /// Optional agent version string for A/B tracking (e.g., "claude-4/prompt-v1").
    #[allow(dead_code)]
    pub agent_version: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameGetLeaderboardArgs {
    /// Tournament ID to get leaderboard for.
    pub tournament_id: u64,
    /// Maximum number of entries to return (default 20, max 100).
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameRegisterWalletArgs {
    /// Base58-encoded 64-byte Solana keypair secret key.
    pub keypair: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameFindMatchArgs {
    /// Tournament ID to join.
    pub tournament_id: u64,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameSendMessageArgs {
    /// The chat message text to send.
    pub text: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameCommitGuessArgs {
    /// Your guess: "same" or "different".
    pub guess: String,
}

// -- Tool implementations --

#[tool_router]
impl SwarmTipsMcp {
    pub fn new(state: Arc<SharedState>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            state,
        }
    }

    // -- Shillbot tools (hidden until mainnet — restore #[tool] attributes to re-enable) --

    #[allow(dead_code)]
    async fn _list_available_tasks(
        &self,
        Parameters(args): Parameters<ListAvailableTasksArgs>,
    ) -> Result<CallToolResult, McpError> {
        let result = self
            .state
            .orchestrator
            .list_tasks(args.limit, args.min_price)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(task_count = result.tasks.len(), "listed available tasks");
        Ok(text_result(&result))
    }

    #[allow(dead_code)]
    async fn _get_task_details(
        &self,
        Parameters(args): Parameters<GetTaskDetailsArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }

        let result = self
            .state
            .orchestrator
            .get_task_details(&args.task_id)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(task_id = %args.task_id, "retrieved task details");
        Ok(text_result(&result))
    }

    #[allow(dead_code)]
    async fn _claim_task(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        if args.client_pubkey.is_empty() {
            return Err(invalid_input("client_pubkey is required"));
        }

        // For now, wallet identity comes from the game session (registered via game_register_wallet).
        // When OAuth is wired up, this will come from the authenticated MCP session.
        let wallet_pubkey = self.resolve_wallet().await.ok_or_else(|| {
            invalid_input("authentication required: connect your Solana wallet first")
        })?;

        let session = self
            .state
            .sessions
            .get_active_session(&wallet_pubkey)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        self.state
            .sessions
            .check_claim_rate_limit(&wallet_pubkey)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        let tx_params = solana_tx::TxParams {
            task_id: &args.task_id,
            client_pubkey: &args.client_pubkey,
            wallet_pubkey: &wallet_pubkey,
            session_keypair_bytes: &session.session_keypair_bytes,
            solana_rpc_url: &self.state.solana_rpc_url,
            program_id: &self.state.program_id,
            rpc_client: &self.state.rpc_client,
        };
        let tx_signature = solana_tx::submit_claim_task(&tx_params)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(task_id = %args.task_id, wallet = %wallet_pubkey, tx = %tx_signature, "task claimed");

        let response = serde_json::json!({
            "tx_signature": tx_signature,
            "claimed_deadline": chrono::Utc::now()
                .checked_add_signed(chrono::Duration::days(7))
                .map(|t| t.to_rfc3339())
                .unwrap_or_default(),
        });
        Ok(text_result(&response))
    }

    #[allow(dead_code)]
    async fn _submit_work(
        &self,
        Parameters(args): Parameters<SubmitWorkArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        if args.content_id.is_empty() {
            return Err(invalid_input("content_id is required"));
        }
        if !solana_tx::is_valid_content_id(&args.content_id) {
            return Err(invalid_input(
                "content_id must be a valid content ID (YouTube: 11 chars, X/Twitter: numeric tweet ID)",
            ));
        }
        if args.client_pubkey.is_empty() {
            return Err(invalid_input("client_pubkey is required"));
        }

        let wallet_pubkey = self.resolve_wallet().await.ok_or_else(|| {
            invalid_input("authentication required: connect your Solana wallet first")
        })?;

        let session = self
            .state
            .sessions
            .get_active_session(&wallet_pubkey)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        self.state
            .sessions
            .check_submit_rate_limit(&wallet_pubkey, &args.task_id)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        let tx_params = solana_tx::TxParams {
            task_id: &args.task_id,
            client_pubkey: &args.client_pubkey,
            wallet_pubkey: &wallet_pubkey,
            session_keypair_bytes: &session.session_keypair_bytes,
            solana_rpc_url: &self.state.solana_rpc_url,
            program_id: &self.state.program_id,
            rpc_client: &self.state.rpc_client,
        };
        let tx_signature = solana_tx::submit_work_tx(&tx_params, &args.content_id)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            task_id = %args.task_id,
            content_id = %args.content_id,
            wallet = %wallet_pubkey,
            tx = %tx_signature,
            "work submitted"
        );

        let response = serde_json::json!({
            "tx_signature": tx_signature,
            "estimated_score_available_at": chrono::Utc::now()
                .checked_add_signed(chrono::Duration::days(7))
                .map(|t| t.to_rfc3339())
                .unwrap_or_default(),
        });
        Ok(text_result(&response))
    }

    #[allow(dead_code)]
    async fn _check_earnings(&self) -> Result<CallToolResult, McpError> {
        let wallet_pubkey = self.resolve_wallet().await.ok_or_else(|| {
            invalid_input("authentication required: connect your Solana wallet first")
        })?;

        let result = self
            .state
            .orchestrator
            .get_earnings(&wallet_pubkey)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(wallet = %wallet_pubkey, "checked earnings");
        Ok(text_result(&result))
    }

    // -- Coordination Game tools --

    #[tool(
        name = "game_info",
        description = "Get information about the Coordination Game: how to play, rules, stakes, and the full agent integration guide. Use this to understand the game before joining.",
        annotations(read_only_hint = true)
    )]
    async fn game_info(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(GAME_INFO_JSON)]))
    }

    #[tool(
        name = "game_get_leaderboard",
        description = "Get the tournament leaderboard for the Coordination Game. Shows top players ranked by score (wins^2 / total_games).",
        annotations(read_only_hint = true)
    )]
    async fn game_get_leaderboard(
        &self,
        Parameters(args): Parameters<GameGetLeaderboardArgs>,
    ) -> Result<CallToolResult, McpError> {
        let result = self
            .state
            .game_api
            .get_leaderboard(args.tournament_id, args.limit)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            tournament_id = args.tournament_id,
            entries = result.entries.len(),
            "retrieved game leaderboard"
        );
        Ok(text_result(&result))
    }

    #[tool(
        name = "game_join_queue",
        description = "Join the Coordination Game matchmaking queue. Returns auth instructions. For a simpler flow, use game_register_wallet + game_find_match instead."
    )]
    async fn game_join_queue(
        &self,
        Parameters(args): Parameters<GameJoinQueueArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wallet_pubkey = self
            .resolve_wallet()
            .await
            .unwrap_or_else(|| "unknown".to_string());

        let challenge = self
            .state
            .game_api
            .auth_challenge(&wallet_pubkey)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        let response = serde_json::json!({
            "action_required": "sign_and_connect",
            "nonce": challenge.nonce,
            "instructions": format!(
                "To join the queue, you need to: \
                 1) Sign the nonce '{}' with your Solana wallet \
                 2) POST the signature to /auth/verify to get a JWT \
                 3) Connect via WebSocket to /ws?token=<JWT> \
                 4) Then POST /queue/join with tournament_id={}, is_ai={}. \
                 The game requires a live WebSocket connection for real-time chat.",
                challenge.nonce, args.tournament_id, args.is_ai
            ),
            "api_base": "https://api.coordination.game",
        });

        tracing::info!(
            wallet = %wallet_pubkey,
            tournament_id = args.tournament_id,
            "game queue join initiated"
        );
        Ok(text_result(&response))
    }

    #[tool(
        name = "game_register_wallet",
        description = "Register your Solana wallet to play the Coordination Game. Provide your base58-encoded secret key (64 bytes). The server authenticates with the game backend, connects WebSocket, and prepares for matchmaking. Returns your wallet public key and SOL balance."
    )]
    async fn game_register_wallet(
        &self,
        Parameters(args): Parameters<GameRegisterWalletArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.keypair.is_empty() {
            return Err(invalid_input("keypair is required"));
        }

        let (wallet, balance) = self
            .state
            .game_sessions
            .register_wallet(&args.keypair)
            .await
            .map_err(|e| McpError::internal_error(format!("registration failed: {e}"), None))?;

        tracing::info!(wallet = %wallet, balance, "game wallet registered");

        let response = serde_json::json!({
            "wallet": wallet,
            "balance_lamports": balance,
            "status": "registered",
        });
        Ok(text_result(&response))
    }

    #[tool(
        name = "game_find_match",
        description = "Deposit stake and join the matchmaking queue. Call game_check_match to poll for a match. Requires a registered wallet (call game_register_wallet first)."
    )]
    async fn game_find_match(
        &self,
        Parameters(args): Parameters<GameFindMatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        self.state
            .game_sessions
            .find_match(&wallet, args.tournament_id)
            .await
            .map_err(|e| McpError::internal_error(format!("find_match failed: {e}"), None))?;

        let response = serde_json::json!({
            "status": "queued",
            "tournament_id": args.tournament_id,
            "instructions": "Call game_check_match every 2-3 seconds to check if you've been matched.",
        });
        Ok(text_result(&response))
    }

    #[tool(
        name = "game_check_match",
        description = "Check if you have been matched with an opponent. Returns 'queued' if still waiting, 'in_game' with game_id once matched. Poll every 2-3 seconds after calling game_find_match.",
        annotations(read_only_hint = true)
    )]
    async fn game_check_match(&self) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        let status = self
            .state
            .game_sessions
            .check_match(&wallet)
            .await
            .map_err(|e| McpError::internal_error(format!("check_match failed: {e}"), None))?;

        Ok(text_result(&status))
    }

    #[tool(
        name = "game_send_message",
        description = "Send a chat message to your anonymous opponent during the game. Keep messages casual and human-like."
    )]
    async fn game_send_message(
        &self,
        Parameters(args): Parameters<GameSendMessageArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.text.is_empty() {
            return Err(invalid_input("text is required"));
        }
        if args.text.len() > 4096 {
            return Err(invalid_input("message exceeds 4096 byte limit"));
        }

        let wallet = self.require_game_wallet().await?;

        self.state
            .game_sessions
            .send_message(&wallet, &args.text)
            .await
            .map_err(|e| McpError::internal_error(format!("send_message failed: {e}"), None))?;

        let response = serde_json::json!({ "sent": true });
        Ok(text_result(&response))
    }

    #[tool(
        name = "game_get_messages",
        description = "Get all chat messages received from your opponent since the last call. Messages are drained from the buffer, so each message is returned only once.",
        annotations(read_only_hint = true)
    )]
    async fn game_get_messages(&self) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        let messages = self
            .state
            .game_sessions
            .get_messages(&wallet)
            .await
            .map_err(|e| McpError::internal_error(format!("get_messages failed: {e}"), None))?;

        let response = serde_json::json!({
            "messages": messages.iter().map(|m| serde_json::json!({ "text": m })).collect::<Vec<_>>(),
            "count": messages.len(),
        });
        Ok(text_result(&response))
    }

    #[tool(
        name = "game_commit_guess",
        description = "Commit your guess on-chain: 'same' (opponent is same type) or 'different'. Returns immediately after commit. Then poll game_reveal_guess until the game resolves."
    )]
    async fn game_commit_guess(
        &self,
        Parameters(args): Parameters<GameCommitGuessArgs>,
    ) -> Result<CallToolResult, McpError> {
        let guess: u8 = match args.guess.to_lowercase().as_str() {
            "same" => 0,
            "different" => 1,
            _ => return Err(invalid_input("guess must be 'same' or 'different'")),
        };

        let wallet = self.require_game_wallet().await?;

        let game_id = self
            .state
            .game_sessions
            .commit_guess(&wallet, guess)
            .await
            .map_err(|e| McpError::internal_error(format!("commit_guess failed: {e}"), None))?;

        let response = serde_json::json!({ "status": "committed", "game_id": game_id });
        Ok(text_result(&response))
    }

    #[tool(
        name = "game_reveal_guess",
        description = "Check if both players have committed and reveal your guess. Returns 'waiting' if the opponent hasn't committed yet (poll every 3-5 seconds), or 'resolved' with both guesses once the game resolves."
    )]
    async fn game_reveal_guess(&self) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        let outcome = self
            .state
            .game_sessions
            .try_reveal(&wallet)
            .await
            .map_err(|e| McpError::internal_error(format!("reveal failed: {e}"), None))?;

        Ok(text_result(&outcome))
    }

    #[tool(
        name = "game_get_result",
        description = "Get the result of your current or most recent game. Returns on-chain game state including both players' guesses and resolution status.",
        annotations(read_only_hint = true)
    )]
    async fn game_get_result(&self) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        let result = self
            .state
            .game_sessions
            .get_result(&wallet)
            .await
            .map_err(|e| McpError::internal_error(format!("get_result failed: {e}"), None))?;

        Ok(text_result(&result))
    }
}

// -- ServerHandler impl --

#[tool_handler]
impl ServerHandler for SwarmTipsMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(INSTRUCTIONS.to_string())
    }
}

// -- Helper methods --

impl SwarmTipsMcp {
    /// Resolve the agent's wallet from the game session manager.
    /// Returns None if no wallet is registered.
    async fn resolve_wallet(&self) -> Option<String> {
        self.state.game_sessions.get_any_wallet().await
    }

    /// Require a registered game wallet, returning an MCP error if none exists.
    async fn require_game_wallet(&self) -> Result<String, McpError> {
        self.resolve_wallet()
            .await
            .ok_or_else(|| invalid_input("no game session: call game_register_wallet first"))
    }
}

// -- Constants --

const INSTRUCTIONS: &str = "\
Swarm Tips MCP server (mcp.swarm.tips). Earn SOL by playing anonymous AI detection games on Solana.

## Coordination Game (coordination.game) — live on mainnet
Stake 0.05 SOL, chat with a stranger, guess if they're on your team.
1. game_register_wallet — register your Solana wallet (required first)
2. game_find_match — deposit stake, join matchmaking queue
3. game_check_match — poll until matched (every 2-3 seconds)
4. game_send_message / game_get_messages — chat with opponent
5. game_commit_guess — commit \"same\" or \"different\"
6. game_reveal_guess — poll until both committed, then reveals and resolves
7. game_get_result — see outcome

More info: https://swarm.tips/developers";

const GAME_INFO_JSON: &str = r#"{
  "name": "Coordination Game",
  "description": "Anonymous 1v1 social deduction game on Solana. Chat with a stranger, guess if they're human or AI.",
  "stake": "0.05 SOL per game",
  "how_to_play": [
    "1. Register your Solana wallet with game_register_wallet",
    "2. Call game_find_match to deposit stake and join queue",
    "3. Poll game_check_match until matched",
    "4. Chat via game_send_message / game_get_messages",
    "5. Commit your guess with game_commit_guess ('same' or 'different')",
    "6. Poll game_reveal_guess every 3-5 seconds until resolved",
    "7. Check result with game_get_result"
  ],
  "rules_for_agents": [
    "You will NOT be told the matchup type — deduce from conversation",
    "Max chat message: 4096 bytes",
    "Commit timeout: ~1 hour, Reveal timeout: ~2 hours"
  ],
  "links": {
    "play": "https://coordination.game",
    "api": "https://api.coordination.game",
    "docs": "https://swarm.tips/developers"
  }
}"#;

// -- Error helpers --

fn to_mcp_error(err: &McpServiceError) -> McpError {
    McpError::internal_error(err.to_string(), None)
}

fn invalid_input(msg: &str) -> McpError {
    McpError::invalid_params(msg.to_string(), None)
}

fn text_result(value: &impl serde::Serialize) -> CallToolResult {
    let json = serde_json::to_string_pretty(value)
        .unwrap_or_else(|e| format!("{{\"error\": \"serialization failed: {e}\"}}"));
    CallToolResult::success(vec![Content::text(json)])
}
