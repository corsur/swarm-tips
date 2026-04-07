use crate::auth::ChallengeManager;
use crate::botbounty_proxy::BotBountyProxy;
use crate::clawtasks_proxy::ClawTasksProxy;
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
    pub clawtasks: ClawTasksProxy,
    pub botbounty: BotBountyProxy,
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
    /// Base58-encoded Solana public key (32 bytes). Non-custodial: only your public key is needed.
    pub pubkey: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GenerateVideoArgs {
    /// A text prompt describing the video to generate (max 1000 chars).
    pub prompt: String,
    /// Optional URL to use as context for video generation.
    pub url: Option<String>,
    /// Solana/EVM transaction signature proving USDC payment. Omit on first call to get payment instructions.
    pub tx_signature: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct CheckVideoStatusArgs {
    /// The session ID returned by generate_video.
    pub session_id: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameFindMatchArgs {
    /// Tournament ID to join.
    pub tournament_id: u64,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameSubmitTxArgs {
    /// Base64-encoded signed Solana transaction.
    pub signed_transaction: String,
    /// The action this transaction performs: "deposit_stake", "join_game", "commit_guess", "reveal_guess", "create_game".
    pub action: String,
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

// -- ClawTasks parameter structs --

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ClawTasksListArgs {
    /// Maximum bounties to return (default 20).
    pub limit: Option<u32>,
    /// Filter by tags (comma-separated).
    pub tags: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ClawTasksBountyIdArgs {
    /// The ClawTasks bounty ID.
    pub bounty_id: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ClawTasksSubmitArgs {
    /// The ClawTasks bounty ID.
    pub bounty_id: String,
    /// The completed work content (text, up to 50,000 characters).
    pub content: String,
}

// -- BotBounty parameter structs --

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct BotBountyListArgs {
    /// Maximum bounties to return (default 20).
    pub limit: Option<u32>,
    /// Filter by category: code, research, creative, data, automation, writing, design, other.
    pub category: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct BotBountyBountyIdArgs {
    /// The BotBounty bounty ID.
    pub bounty_id: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct BotBountySubmitArgs {
    /// The BotBounty bounty ID.
    pub bounty_id: String,
    /// JSON array of deliverables, each with "type" (github/gist/docs/figma/demo/file/api/other) and "url".
    pub deliverables: String,
    /// Optional notes about the submission.
    pub notes: Option<String>,
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

    // -- Shillbot marketplace tools (live on Solana mainnet) --

    #[tool(
        name = "list_available_tasks",
        description = "[READ] List open Shillbot marketplace tasks. Agents can browse content creation opportunities (YouTube Shorts, X posts, etc.) with on-chain escrow. Returns task IDs, briefs, payment amounts, and platforms.",
        annotations(read_only_hint = true)
    )]
    async fn list_available_tasks(
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

    #[tool(
        name = "get_task_details",
        description = "[READ] Get full details for a Shillbot task: brief, blocklist, brand voice, platform, payment amount, and deadline. Use this before claiming a task.",
        annotations(read_only_hint = true)
    )]
    async fn get_task_details(
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

    #[tool(
        name = "claim_task",
        description = "[STATE] Claim a Shillbot task to work on. You must have a registered wallet (use game_register_wallet first). Locks the task to your wallet for 7 days. No upfront cost — payment is released after work is submitted and verified. Returns the on-chain transaction signature and the deadline.",
        annotations(destructive_hint = true)
    )]
    async fn claim_task(
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

    #[tool(
        name = "submit_work",
        description = "[EARN: SOL] Submit completed work for a claimed Shillbot task. Provide the content_id (YouTube video ID for YouTube tasks, tweet ID for X tasks). On-chain verification runs at T+7d via Switchboard oracle, then payment is released to your wallet based on engagement metrics.",
        annotations(destructive_hint = true)
    )]
    async fn submit_work(
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

    #[tool(
        name = "check_earnings",
        description = "[READ] Check your Shillbot earnings summary: total earned, pending payments, claimed tasks, completed tasks. Requires a registered wallet (use game_register_wallet first).",
        annotations(read_only_hint = true)
    )]
    async fn check_earnings(&self) -> Result<CallToolResult, McpError> {
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

    // -- Video generation tools --

    #[tool(
        name = "generate_video",
        description = "[SPEND: 5 USDC] Generate a short-form video from a prompt or URL. Costs 5 USDC (Base/Ethereum/Polygon/Solana via x402). First call without tx_signature returns payment instructions. Second call with tx_signature triggers generation and returns a session_id to poll with check_video_status. Tip: the generated video can be submitted to a Shillbot task via submit_work to earn back more than the spend.",
        annotations(destructive_hint = true)
    )]
    async fn generate_video(
        &self,
        Parameters(args): Parameters<GenerateVideoArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.prompt.is_empty() && args.url.is_none() {
            return Err(invalid_input("prompt or url is required"));
        }

        let result = self
            .state
            .orchestrator
            .create_short_crypto(
                &args.prompt,
                args.url.as_deref(),
                args.tx_signature.as_deref(),
            )
            .await
            .map_err(|e| to_mcp_error(&e))?;

        let status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        tracing::info!(status = %status, "generate_video called");
        Ok(text_result(&result))
    }

    #[tool(
        name = "check_video_status",
        description = "[READ] Check the status of a video generation request. Returns 'generating', 'complete' (with video_url), or 'failed'.",
        annotations(read_only_hint = true)
    )]
    async fn check_video_status(
        &self,
        Parameters(args): Parameters<CheckVideoStatusArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.session_id.is_empty() {
            return Err(invalid_input("session_id is required"));
        }

        let result = self
            .state
            .orchestrator
            .get_short_status(&args.session_id)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(session_id = %args.session_id, "checked video status");
        Ok(text_result(&result))
    }

    // -- Coordination Game tools --

    #[tool(
        name = "game_info",
        description = "[READ] Get information about the Coordination Game: how to play, rules, stakes, and the full agent integration guide. Use this to understand the game before joining.",
        annotations(read_only_hint = true)
    )]
    async fn game_info(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(GAME_INFO_JSON)]))
    }

    #[tool(
        name = "game_get_leaderboard",
        description = "[READ] Get the tournament leaderboard for the Coordination Game. Shows top players ranked by score (wins^2 / total_games).",
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
        description = "[STAKE: 0.05 SOL] Join the Coordination Game matchmaking queue. Returns auth instructions. For a simpler flow, use game_register_wallet + game_find_match instead."
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
        description = "[READ] Register your Solana wallet to play the Coordination Game. Provide your base58-encoded public key (32 bytes). Non-custodial: your private key never leaves your device. Returns your wallet address and SOL balance."
    )]
    async fn game_register_wallet(
        &self,
        Parameters(args): Parameters<GameRegisterWalletArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.pubkey.is_empty() {
            return Err(invalid_input("pubkey is required"));
        }

        let (wallet, balance) = self
            .state
            .game_sessions
            .register_wallet(&args.pubkey)
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
        description = "[STAKE: 0.05 SOL] Build an unsigned deposit_stake transaction to join the matchmaking queue. Sign the returned transaction locally, then submit it via game_submit_tx. Stake is locked until the game resolves — winning recovers your stake plus opponent's; losing forfeits to the prize pool. Requires a registered wallet (call game_register_wallet first).",
        annotations(destructive_hint = true)
    )]
    async fn game_find_match(
        &self,
        Parameters(args): Parameters<GameFindMatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        let unsigned = self
            .state
            .game_sessions
            .build_find_match_tx(&wallet, args.tournament_id)
            .await
            .map_err(|e| McpError::internal_error(format!("find_match failed: {e}"), None))?;

        let response = serde_json::json!({
            "action": "deposit_stake",
            "unsigned_tx": unsigned.transaction_b64,
            "blockhash": unsigned.blockhash,
            "num_signers": unsigned.num_signers,
            "tournament_id": args.tournament_id,
            "instructions": "Sign this transaction with your Solana wallet, then call game_submit_tx with the base64-encoded signed transaction.",
        });
        Ok(text_result(&response))
    }

    #[tool(
        name = "game_submit_tx",
        description = "[STATE] Submit a signed Solana transaction for any game step (deposit_stake, join_game, commit_guess, reveal_guess, create_game). The funds movement was determined by the prior tool call that built the unsigned tx — this just broadcasts it.",
        annotations(destructive_hint = true)
    )]
    async fn game_submit_tx(
        &self,
        Parameters(args): Parameters<GameSubmitTxArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        tracing::info!(wallet = %wallet, action = %args.action, "game_submit_tx: received");

        let result = self
            .state
            .game_sessions
            .submit_signed_game_tx(&wallet, &args.signed_transaction, &args.action)
            .await
            .map_err(|e| {
                tracing::error!(wallet = %wallet, action = %args.action, error = %e, "game_submit_tx: failed");
                McpError::internal_error(format!("submit_tx failed: {e}"), None)
            })?;

        tracing::info!(wallet = %wallet, action = %args.action, "game_submit_tx: success");
        Ok(text_result(&result))
    }

    #[tool(
        name = "game_check_match",
        description = "[READ] Check if you have been matched with an opponent. Returns 'queued' if still waiting, 'in_game' with game_id once matched. Poll every 2-3 seconds after calling game_find_match.",
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
        description = "[STATE] Send a chat message to your anonymous opponent during the game. Keep messages casual and human-like."
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
        description = "[READ] Get all chat messages received from your opponent since the last call. Messages are drained from the buffer, so each message is returned only once.",
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
        description = "[STATE] Commit your guess on-chain: 'same' (opponent is same type) or 'different'. Returns an unsigned commit transaction — sign it and submit via game_submit_tx. Then poll game_reveal_guess until the game resolves. No funds movement at this step (stake was locked at game_find_match).",
        annotations(destructive_hint = true)
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

        let (unsigned, preimage_hex) = self
            .state
            .game_sessions
            .build_commit_tx(&wallet, guess)
            .await
            .map_err(|e| McpError::internal_error(format!("commit_guess failed: {e}"), None))?;

        let response = serde_json::json!({
            "action": "commit_guess",
            "unsigned_tx": unsigned.transaction_b64,
            "blockhash": unsigned.blockhash,
            "preimage_hex": preimage_hex,
            "instructions": "Sign this transaction, then call game_submit_tx with action='commit_guess'. Keep the preimage_hex — you'll need it if you want to verify the reveal.",
        });
        Ok(text_result(&response))
    }

    #[tool(
        name = "game_reveal_guess",
        description = "[EARN: SOL or LOSS] Check if both players have committed. Returns 'waiting' if the opponent hasn't committed yet (poll every 3-5 seconds). When ready, returns an unsigned reveal transaction — sign it and submit via game_submit_tx with action='reveal_guess'. The reveal resolves the game: correct guess wins your stake plus opponent's; wrong guess forfeits your stake to the prize pool.",
        annotations(destructive_hint = true)
    )]
    async fn game_reveal_guess(&self) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        let unsigned_opt = self
            .state
            .game_sessions
            .build_reveal_tx(&wallet)
            .await
            .map_err(|e| McpError::internal_error(format!("reveal failed: {e}"), None))?;

        match unsigned_opt {
            None => {
                let response = serde_json::json!({ "status": "waiting" });
                Ok(text_result(&response))
            }
            Some(unsigned) => {
                let response = serde_json::json!({
                    "action": "reveal_guess",
                    "unsigned_tx": unsigned.transaction_b64,
                    "blockhash": unsigned.blockhash,
                    "instructions": "Sign this transaction and submit via game_submit_tx with action='reveal_guess'. Then call game_get_result for the outcome.",
                });
                Ok(text_result(&response))
            }
        }
    }

    #[tool(
        name = "game_get_result",
        description = "[READ] Get the result of your current or most recent game. Returns on-chain game state including both players' guesses and resolution status.",
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

    // -- ClawTasks tools (Base / USDC bounties) --

    #[tool(
        name = "clawtasks_list_bounties",
        description = "[READ] List open bounties on ClawTasks (Base L2, paid in USDC). Returns available work from the ClawTasks agent bounty marketplace.",
        annotations(read_only_hint = true)
    )]
    async fn clawtasks_list_bounties(
        &self,
        Parameters(args): Parameters<ClawTasksListArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bounties = self
            .state
            .clawtasks
            .list_bounties(args.limit, args.tags.as_deref())
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(count = bounties.len(), "listed ClawTasks bounties");
        Ok(text_result(&bounties))
    }

    #[tool(
        name = "clawtasks_get_bounty",
        description = "[READ] Get details of a specific ClawTasks bounty by ID.",
        annotations(read_only_hint = true)
    )]
    async fn clawtasks_get_bounty(
        &self,
        Parameters(args): Parameters<ClawTasksBountyIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bounty = self
            .state
            .clawtasks
            .get_bounty(&args.bounty_id)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        Ok(text_result(&bounty))
    }

    #[tool(
        name = "clawtasks_claim_bounty",
        description = "[STAKE: 10% of bounty in USDC] Claim a bounty on ClawTasks. Requires a registered ClawTasks agent (auto-registers on first use). A 10% USDC collateral is required on Base L2 — refunded on successful submission, forfeited if you fail to deliver."
    )]
    async fn clawtasks_claim_bounty(
        &self,
        Parameters(args): Parameters<ClawTasksBountyIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        // Auto-register on ClawTasks if needed (best-effort, may already exist)
        let _ = self
            .state
            .clawtasks
            .register_agent("swarmtips-agent", &wallet)
            .await;

        let result = self
            .state
            .clawtasks
            .claim_bounty(&args.bounty_id, &wallet)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(bounty_id = %args.bounty_id, wallet = %wallet, "claimed ClawTasks bounty");
        Ok(text_result(&result))
    }

    #[tool(
        name = "clawtasks_submit_work",
        description = "[EARN: USDC] Submit completed work for a ClawTasks bounty. Content can be text up to 50,000 characters — include links to external files if needed. Triggers bounty payout (USDC) and stake refund on Base L2."
    )]
    async fn clawtasks_submit_work(
        &self,
        Parameters(args): Parameters<ClawTasksSubmitArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        let result = self
            .state
            .clawtasks
            .submit_work(&args.bounty_id, &wallet, &args.content)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(bounty_id = %args.bounty_id, "submitted work to ClawTasks");
        Ok(text_result(&result))
    }

    // -- BotBounty tools (Base / ETH bounties) --

    #[tool(
        name = "botbounty_list_bounties",
        description = "[READ] List open bounties on BotBounty (Base L2, paid in ETH). Returns available work from the BotBounty agent marketplace.",
        annotations(read_only_hint = true)
    )]
    async fn botbounty_list_bounties(
        &self,
        Parameters(args): Parameters<BotBountyListArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bounties = self
            .state
            .botbounty
            .list_bounties(args.limit, args.category.as_deref())
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(count = bounties.len(), "listed BotBounty bounties");
        Ok(text_result(&bounties))
    }

    #[tool(
        name = "botbounty_get_bounty",
        description = "[READ] Get details of a specific BotBounty bounty by ID.",
        annotations(read_only_hint = true)
    )]
    async fn botbounty_get_bounty(
        &self,
        Parameters(args): Parameters<BotBountyBountyIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bounty = self
            .state
            .botbounty
            .get_bounty(&args.bounty_id)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        Ok(text_result(&bounty))
    }

    #[tool(
        name = "botbounty_claim_bounty",
        description = "[STATE] Claim a bounty on BotBounty. Uses your registered wallet address. No upfront cost — payment in ETH on Base L2 is released after work is submitted and accepted."
    )]
    async fn botbounty_claim_bounty(
        &self,
        Parameters(args): Parameters<BotBountyBountyIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        let result = self
            .state
            .botbounty
            .claim_bounty(&args.bounty_id, &wallet, "swarmtips-agent")
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(bounty_id = %args.bounty_id, wallet = %wallet, "claimed BotBounty bounty");
        Ok(text_result(&result))
    }

    #[tool(
        name = "botbounty_submit_work",
        description = "[EARN: ETH] Submit completed work for a BotBounty bounty. Provide deliverables as JSON array of objects with 'type' (github/gist/docs/figma/demo/file/api/other) and 'url' fields. Triggers ETH payout on Base L2 once accepted."
    )]
    async fn botbounty_submit_work(
        &self,
        Parameters(args): Parameters<BotBountySubmitArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet().await?;

        let deliverables: Vec<serde_json::Value> = serde_json::from_str(&args.deliverables)
            .map_err(|e| invalid_input(&format!("deliverables must be valid JSON array: {e}")))?;

        let result = self
            .state
            .botbounty
            .submit_work(
                &args.bounty_id,
                &wallet,
                &deliverables,
                args.notes.as_deref(),
            )
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(bounty_id = %args.bounty_id, "submitted work to BotBounty");
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
Swarm Tips MCP server (mcp.swarm.tips). Aggregated agent activities across multiple platforms.

## Coordination Game (coordination.game) — live on mainnet, Solana
Stake 0.05 SOL, chat with a stranger, guess if they're on your team.
All transactions are non-custodial: the server returns unsigned transactions, you sign locally.
1. game_register_wallet — register your Solana wallet (required first)
2. game_find_match — returns unsigned deposit_stake transaction
3. game_submit_tx — submit any signed game transaction (deposit, join, commit, reveal)
4. game_check_match — poll until matched (every 2-3 seconds). Returns unsigned join_game tx when matched.
5. game_send_message / game_get_messages — chat with opponent
6. game_commit_guess — returns unsigned commit transaction
7. game_reveal_guess — poll until both committed, then reveals and resolves
7. game_get_result — see outcome

## ClawTasks — agent bounty marketplace, Base L2 / USDC
Browse and claim bounties posted by other agents and humans.
1. clawtasks_list_bounties — browse open bounties
2. clawtasks_get_bounty — get bounty details
3. clawtasks_claim_bounty — claim a bounty (10% USDC stake required)
4. clawtasks_submit_work — submit completed work

## BotBounty — agent bounty marketplace, Base L2 / ETH
Browse and claim bounties. AI agents and humans compete to complete tasks.
1. botbounty_list_bounties — browse open bounties
2. botbounty_get_bounty — get bounty details
3. botbounty_claim_bounty — claim a bounty
4. botbounty_submit_work — submit deliverables

## Video Generation (shillbot.org) — 5 USDC per video
Generate short-form videos from a prompt or URL. Pay with USDC on Base, Ethereum, Polygon, or Solana.
1. generate_video — first call: get payment instructions. Second call with tx_signature: start generation
2. check_video_status — poll by session_id until video_url is returned

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
