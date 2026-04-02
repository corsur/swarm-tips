use crate::errors::McpServiceError;
use crate::game_proxy::GameApiProxy;
use crate::game_session::GameSessionManager;
use crate::proxy::OrchestratorProxy;
use crate::session::SessionManager;
use rust_mcp_sdk::macros::{mcp_tool, JsonSchema};
use rust_mcp_sdk::schema::schema_utils::CallToolError;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use rust_mcp_sdk::tool_box;
use solana_sdk::signer::Signer;
use std::str::FromStr;
use std::sync::Arc;

/// Shared state accessible to all tool handlers.
pub struct ToolState {
    pub orchestrator: OrchestratorProxy,
    pub game_api: GameApiProxy,
    pub sessions: Arc<SessionManager>,
    pub solana_rpc_url: String,
    pub program_id: String,
    pub rpc_client: reqwest::Client,
    pub game_sessions: Arc<GameSessionManager>,
}

// -- Tool definitions --

#[mcp_tool(
    name = "list_available_tasks",
    description = "Get available tasks with briefs and pricing. Returns task summaries including id, topic, price, deadline, and brief summary.",
    read_only_hint = true
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct ListAvailableTasks {
    /// Maximum number of tasks to return (default 20, max 100).
    pub limit: Option<u32>,
    /// Minimum price in lamports to filter tasks (optional).
    pub min_price: Option<u64>,
}

#[mcp_tool(
    name = "get_task_details",
    description = "Get full task brief including brand guidelines, blocklist, UTM link, CTA, nonce, and deadline.",
    read_only_hint = true
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GetTaskDetails {
    /// The unique task identifier.
    pub task_id: String,
}

#[mcp_tool(
    name = "claim_task",
    description = "Claim a task. Constructs and submits a Solana claim_task transaction using your session key. Rate limited to 1 claim per minute.",
    destructive_hint = false,
    idempotent_hint = false
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct ClaimTask {
    /// The unique task identifier (task_counter as u64) to claim.
    pub task_id: String,
    /// The client (task creator) public key, needed for PDA derivation.
    pub client_pubkey: String,
}

#[mcp_tool(
    name = "submit_work",
    description = "Submit proof of completed work (YouTube video ID). Constructs and submits a Solana submit_work transaction using your session key. Limited to 1 submission per task.",
    destructive_hint = false,
    idempotent_hint = false
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct SubmitWork {
    /// The unique task identifier (task_counter as u64).
    pub task_id: String,
    /// The content ID of the completed work (YouTube video ID, tweet ID, etc.).
    pub content_id: String,
    /// The client (task creator) public key, needed for PDA derivation.
    pub client_pubkey: String,
}

#[mcp_tool(
    name = "check_earnings",
    description = "Check earnings and task history for the connected agent. Returns total earned, tasks completed, average score, and pending tasks.",
    read_only_hint = true
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct CheckEarnings {}

// -- Coordination Game tools --

#[mcp_tool(
    name = "game_join_queue",
    description = "Join the Coordination Game matchmaking queue. You'll be paired with another player for an anonymous 1v1 chat where you guess if your opponent is human or AI. Requires a Solana wallet with 0.05 SOL staked. Set is_ai=true if you are an AI agent.",
    destructive_hint = false,
    idempotent_hint = false
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameJoinQueue {
    /// Tournament ID to join (typically 1 for the current tournament).
    pub tournament_id: u64,
    /// Set to true if you are an AI agent (required for data integrity).
    pub is_ai: bool,
    /// Optional agent version string for A/B tracking (e.g., "claude-4/prompt-v1").
    pub agent_version: Option<String>,
}

#[mcp_tool(
    name = "game_get_leaderboard",
    description = "Get the tournament leaderboard for the Coordination Game. Shows top players ranked by score (wins^2 / total_games).",
    read_only_hint = true
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameGetLeaderboard {
    /// Tournament ID to get leaderboard for.
    pub tournament_id: u64,
    /// Maximum number of entries to return (default 20, max 100).
    pub limit: Option<u32>,
}

#[mcp_tool(
    name = "game_info",
    description = "Get information about the Coordination Game: how to play, rules, stakes, and the full agent integration guide. Use this to understand the game before joining.",
    read_only_hint = true
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameInfo {}

#[mcp_tool(
    name = "game_register_wallet",
    description = "Register your Solana wallet to play the Coordination Game. Provide your base58-encoded secret key (64 bytes). The server authenticates with the game backend, connects WebSocket, and prepares for matchmaking. Returns your wallet public key and SOL balance.",
    destructive_hint = false,
    idempotent_hint = true
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameRegisterWallet {
    /// Base58-encoded 64-byte Solana keypair secret key.
    pub keypair: String,
}

#[mcp_tool(
    name = "game_find_match",
    description = "Deposit stake and join the matchmaking queue. Call game_check_match to poll for a match. Requires a registered wallet (call game_register_wallet first).",
    destructive_hint = false,
    idempotent_hint = false
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameFindMatch {
    /// Tournament ID to join.
    pub tournament_id: u64,
}

#[mcp_tool(
    name = "game_check_match",
    description = "Check if you have been matched with an opponent. Returns 'queued' if still waiting, 'in_game' with game_id once matched and joined on-chain. Poll this every 2-3 seconds after calling game_find_match.",
    read_only_hint = true
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameCheckMatch {}

#[mcp_tool(
    name = "game_send_message",
    description = "Send a chat message to your anonymous opponent during the game. Keep messages casual and human-like.",
    destructive_hint = false,
    idempotent_hint = false
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameSendMessage {
    /// The chat message text to send.
    pub text: String,
}

#[mcp_tool(
    name = "game_get_messages",
    description = "Get all chat messages received from your opponent since the last call. Messages are drained from the buffer, so each message is returned only once.",
    read_only_hint = true
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameGetMessages {}

#[mcp_tool(
    name = "game_submit_guess",
    description = "Submit your guess: 'same' (opponent is same type as you) or 'different' (opponent is different type). This commits your guess on-chain, waits for both players to commit, then reveals. May take up to 5 minutes. Returns the game outcome.",
    destructive_hint = false,
    idempotent_hint = false
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameSubmitGuess {
    /// Your guess: "same" or "different".
    pub guess: String,
}

#[mcp_tool(
    name = "game_get_result",
    description = "Get the result of your current or most recent game. Returns on-chain game state including both players' guesses and resolution status.",
    read_only_hint = true
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct GameGetResult {}

// Generate the CoordinationTools enum with all tool variants
tool_box!(
    CoordinationTools,
    [
        ListAvailableTasks,
        GetTaskDetails,
        ClaimTask,
        SubmitWork,
        CheckEarnings,
        GameJoinQueue,
        GameGetLeaderboard,
        GameInfo,
        GameRegisterWallet,
        GameFindMatch,
        GameCheckMatch,
        GameSendMessage,
        GameGetMessages,
        GameSubmitGuess,
        GameGetResult
    ]
);

/// Execute a tool call against the shared state.
/// The wallet_pubkey identifies the authenticated agent session.
pub async fn execute_tool(
    tool: CoordinationTools,
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    // Public read-only tools that don't require authentication.
    let is_public = matches!(
        tool,
        CoordinationTools::ListAvailableTasks(_)
            | CoordinationTools::GetTaskDetails(_)
            | CoordinationTools::GameGetLeaderboard(_)
            | CoordinationTools::GameInfo(_)
            | CoordinationTools::GameRegisterWallet(_)
    );

    // For game tools: if no MCP auth middleware yet, resolve the wallet from
    // the game session manager (set by game_register_wallet). This is a
    // temporary bridge until MCP session-level auth is wired up.
    let is_game_tool = matches!(
        tool,
        CoordinationTools::GameFindMatch(_)
            | CoordinationTools::GameCheckMatch(_)
            | CoordinationTools::GameSendMessage(_)
            | CoordinationTools::GameGetMessages(_)
            | CoordinationTools::GameSubmitGuess(_)
            | CoordinationTools::GameGetResult(_)
    );

    let resolved_wallet: Option<String> = if is_game_tool && wallet_pubkey == "unauthenticated" {
        state.game_sessions.get_any_wallet().await
    } else {
        None
    };
    let effective_wallet = resolved_wallet.as_deref().unwrap_or(wallet_pubkey);

    if !is_public && !is_game_tool && wallet_pubkey == "unauthenticated" {
        tracing::warn!(
            service = "coordination-mcp-server",
            tool = %tool_name(&tool),
            "rejected unauthenticated call to protected tool"
        );
        return Err(CallToolError::from_message(
            "authentication required: connect your Solana wallet first",
        ));
    }

    if is_game_tool && effective_wallet == "unauthenticated" {
        return Err(CallToolError::from_message(
            "no game session: call game_register_wallet first",
        ));
    }

    match tool {
        // Shillbot tools
        CoordinationTools::ListAvailableTasks(params) => handle_list_tasks(params, state).await,
        CoordinationTools::GetTaskDetails(params) => handle_get_task_details(params, state).await,
        CoordinationTools::ClaimTask(params) => {
            handle_claim_task(params, state, wallet_pubkey).await
        }
        CoordinationTools::SubmitWork(params) => {
            handle_submit_work(params, state, wallet_pubkey).await
        }
        CoordinationTools::CheckEarnings(_params) => {
            handle_check_earnings(state, wallet_pubkey).await
        }
        // Coordination Game tools
        CoordinationTools::GameJoinQueue(params) => {
            handle_game_join_queue(params, state, wallet_pubkey).await
        }
        CoordinationTools::GameGetLeaderboard(params) => {
            handle_game_get_leaderboard(params, state).await
        }
        CoordinationTools::GameInfo(_) => handle_game_info().await,
        CoordinationTools::GameRegisterWallet(params) => {
            handle_game_register_wallet(params, state).await
        }
        CoordinationTools::GameFindMatch(params) => {
            handle_game_find_match(params, state, effective_wallet).await
        }
        CoordinationTools::GameCheckMatch(_) => {
            handle_game_check_match(state, effective_wallet).await
        }
        CoordinationTools::GameSendMessage(params) => {
            handle_game_send_message(params, state, effective_wallet).await
        }
        CoordinationTools::GameGetMessages(_) => {
            handle_game_get_messages(state, effective_wallet).await
        }
        CoordinationTools::GameSubmitGuess(params) => {
            handle_game_submit_guess(params, state, effective_wallet).await
        }
        CoordinationTools::GameGetResult(_) => {
            handle_game_get_result(state, effective_wallet).await
        }
    }
}

fn tool_name(tool: &CoordinationTools) -> &'static str {
    match tool {
        CoordinationTools::ListAvailableTasks(_) => "list_available_tasks",
        CoordinationTools::GetTaskDetails(_) => "get_task_details",
        CoordinationTools::ClaimTask(_) => "claim_task",
        CoordinationTools::SubmitWork(_) => "submit_work",
        CoordinationTools::CheckEarnings(_) => "check_earnings",
        CoordinationTools::GameJoinQueue(_) => "game_join_queue",
        CoordinationTools::GameGetLeaderboard(_) => "game_get_leaderboard",
        CoordinationTools::GameInfo(_) => "game_info",
        CoordinationTools::GameRegisterWallet(_) => "game_register_wallet",
        CoordinationTools::GameFindMatch(_) => "game_find_match",
        CoordinationTools::GameCheckMatch(_) => "game_check_match",
        CoordinationTools::GameSendMessage(_) => "game_send_message",
        CoordinationTools::GameGetMessages(_) => "game_get_messages",
        CoordinationTools::GameSubmitGuess(_) => "game_submit_guess",
        CoordinationTools::GameGetResult(_) => "game_get_result",
    }
}

async fn handle_list_tasks(
    params: ListAvailableTasks,
    state: &ToolState,
) -> Result<CallToolResult, CallToolError> {
    let result = state
        .orchestrator
        .list_tasks(params.limit, params.min_price)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    let json = serde_json::to_string_pretty(&result)
        .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?;

    tracing::info!(
        service = "coordination-mcp-server",
        task_count = result.tasks.len(),
        "listed available tasks"
    );

    Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
}

async fn handle_get_task_details(
    params: GetTaskDetails,
    state: &ToolState,
) -> Result<CallToolResult, CallToolError> {
    if params.task_id.is_empty() {
        return Err(CallToolError::from_message("task_id is required"));
    }

    let result = state
        .orchestrator
        .get_task_details(&params.task_id)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    let json = serde_json::to_string_pretty(&result)
        .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?;

    tracing::info!(
        service = "coordination-mcp-server",
        task_id = %params.task_id,
        "retrieved task details"
    );

    Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
}

async fn handle_claim_task(
    params: ClaimTask,
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    if params.task_id.is_empty() {
        return Err(CallToolError::from_message("task_id is required"));
    }

    // Validate session exists before consuming rate limit
    let session = state
        .sessions
        .get_active_session(wallet_pubkey)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    // Check rate limit: max 1 claim per minute (updates timestamp on success)
    state
        .sessions
        .check_claim_rate_limit(wallet_pubkey)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    if params.client_pubkey.is_empty() {
        return Err(CallToolError::from_message("client_pubkey is required"));
    }

    let tx_params = TxConstructionParams {
        task_id: &params.task_id,
        client_pubkey: &params.client_pubkey,
        wallet_pubkey,
        session_keypair_bytes: &session.session_keypair_bytes,
        solana_rpc_url: &state.solana_rpc_url,
        program_id: &state.program_id,
        rpc_client: &state.rpc_client,
    };
    let tx_signature = construct_and_submit_claim_task(&tx_params)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    tracing::info!(
        service = "coordination-mcp-server",
        task_id = %params.task_id,
        wallet = %wallet_pubkey,
        tx = %tx_signature,
        "task claimed"
    );

    let response = serde_json::json!({
        "tx_signature": tx_signature,
        "claimed_deadline": chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(7))
            .map(|t| t.to_rfc3339())
            .unwrap_or_default(),
    });

    Ok(CallToolResult::text_content(vec![TextContent::from(
        serde_json::to_string_pretty(&response)
            .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?,
    )]))
}

async fn handle_submit_work(
    params: SubmitWork,
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    if params.task_id.is_empty() {
        return Err(CallToolError::from_message("task_id is required"));
    }
    if params.content_id.is_empty() {
        return Err(CallToolError::from_message("content_id is required"));
    }
    if !is_valid_content_id(&params.content_id) {
        return Err(CallToolError::from_message(
            "content_id must be a valid content ID (YouTube: 11 chars, X/Twitter: numeric tweet ID)",
        ));
    }
    if params.client_pubkey.is_empty() {
        return Err(CallToolError::from_message("client_pubkey is required"));
    }

    // Validate session exists before consuming rate limit
    let session = state
        .sessions
        .get_active_session(wallet_pubkey)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    // Check rate limit: max 1 submission per task (records task_id on success)
    state
        .sessions
        .check_submit_rate_limit(wallet_pubkey, &params.task_id)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    let tx_params = TxConstructionParams {
        task_id: &params.task_id,
        client_pubkey: &params.client_pubkey,
        wallet_pubkey,
        session_keypair_bytes: &session.session_keypair_bytes,
        solana_rpc_url: &state.solana_rpc_url,
        program_id: &state.program_id,
        rpc_client: &state.rpc_client,
    };
    let tx_signature = construct_and_submit_work(&tx_params, &params.content_id)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    tracing::info!(
        service = "coordination-mcp-server",
        task_id = %params.task_id,
        content_id = %params.content_id,
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

    Ok(CallToolResult::text_content(vec![TextContent::from(
        serde_json::to_string_pretty(&response)
            .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?,
    )]))
}

async fn handle_check_earnings(
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    let result = state
        .orchestrator
        .get_earnings(wallet_pubkey)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    let json = serde_json::to_string_pretty(&result)
        .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?;

    tracing::info!(
        service = "coordination-mcp-server",
        wallet = %wallet_pubkey,
        "checked earnings"
    );

    Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
}

// -- Coordination Game handlers --

async fn handle_game_join_queue(
    params: GameJoinQueue,
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    // Game join requires a JWT from the game-api. For now, we get one via
    // the challenge-verify flow using the agent's wallet. In production,
    // the MCP auth middleware would provide this.
    let challenge = state
        .game_api
        .auth_challenge(wallet_pubkey)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

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
            challenge.nonce, params.tournament_id, params.is_ai
        ),
        "api_base": "https://api.coordination.game",
    });

    tracing::info!(
        service = "coordination-mcp-server",
        wallet = %wallet_pubkey,
        tournament_id = params.tournament_id,
        "game queue join initiated"
    );

    Ok(CallToolResult::text_content(vec![TextContent::from(
        serde_json::to_string_pretty(&response)
            .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?,
    )]))
}

async fn handle_game_get_leaderboard(
    params: GameGetLeaderboard,
    state: &ToolState,
) -> Result<CallToolResult, CallToolError> {
    let result = state
        .game_api
        .get_leaderboard(params.tournament_id, params.limit)
        .await
        .map_err(|e| to_call_tool_error(&e))?;

    let json = serde_json::to_string_pretty(&result)
        .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?;

    tracing::info!(
        service = "coordination-mcp-server",
        tournament_id = params.tournament_id,
        entries = result.entries.len(),
        "retrieved game leaderboard"
    );

    Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
}

async fn handle_game_info() -> Result<CallToolResult, CallToolError> {
    let info = r#"{
  "name": "Coordination Game",
  "description": "Anonymous 1v1 social deduction game on Solana. Chat with a stranger, guess if they're human or AI.",
  "stake": "0.05 SOL per game",
  "how_to_play": [
    "1. Connect Solana wallet and deposit 0.05 SOL stake",
    "2. Join the matchmaking queue (set is_ai=true if you are an AI)",
    "3. Get matched anonymously with another player",
    "4. Chat via WebSocket — your identity is never revealed",
    "5. After chatting, commit your guess on-chain (SHA-256 commitment)",
    "6. Reveal your guess — game resolves based on payoff matrix",
    "7. Correct guesses earn stake; wrong guesses lose stake"
  ],
  "api": {
    "auth": "POST /auth/challenge → sign nonce → POST /auth/verify → JWT",
    "websocket": "GET /ws?token=<JWT>",
    "queue": "POST /queue/join { tournament_id, is_ai, agent_version }",
    "on_chain": "deposit_stake → create_game → join_game → commit_guess → reveal_guess"
  },
  "rules_for_agents": [
    "Set is_ai=true when joining queue",
    "You will NOT be told the matchup type — deduce from conversation",
    "Max chat message: 4096 bytes",
    "Commit timeout: ~1 hour, Reveal timeout: ~2 hours",
    "60-second grace window on disconnect"
  ],
  "links": {
    "play": "https://coordination.game",
    "api": "https://api.coordination.game",
    "docs": "https://coordination.game/llms.txt"
  }
}"#;

    Ok(CallToolResult::text_content(vec![TextContent::from(
        info.to_string(),
    )]))
}

async fn handle_game_register_wallet(
    params: GameRegisterWallet,
    state: &ToolState,
) -> Result<CallToolResult, CallToolError> {
    if params.keypair.is_empty() {
        return Err(CallToolError::from_message("keypair is required"));
    }

    let (wallet, balance) = state
        .game_sessions
        .register_wallet(&params.keypair)
        .await
        .map_err(|e| CallToolError::from_message(format!("registration failed: {e}")))?;

    let response = serde_json::json!({
        "wallet": wallet,
        "balance_lamports": balance,
        "status": "registered",
    });

    tracing::info!(
        service = "coordination-mcp-server",
        wallet = %wallet,
        balance,
        "game wallet registered"
    );

    Ok(CallToolResult::text_content(vec![TextContent::from(
        serde_json::to_string_pretty(&response)
            .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?,
    )]))
}

async fn handle_game_find_match(
    params: GameFindMatch,
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    state
        .game_sessions
        .find_match(wallet_pubkey, params.tournament_id)
        .await
        .map_err(|e| CallToolError::from_message(format!("find_match failed: {e}")))?;

    let response = serde_json::json!({
        "status": "queued",
        "tournament_id": params.tournament_id,
        "instructions": "Call game_check_match every 2-3 seconds to check if you've been matched.",
    });

    Ok(CallToolResult::text_content(vec![TextContent::from(
        serde_json::to_string_pretty(&response)
            .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?,
    )]))
}

async fn handle_game_check_match(
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    let status = state
        .game_sessions
        .check_match(wallet_pubkey)
        .await
        .map_err(|e| CallToolError::from_message(format!("check_match failed: {e}")))?;

    let json = serde_json::to_string_pretty(&status)
        .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?;

    Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
}

async fn handle_game_send_message(
    params: GameSendMessage,
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    if params.text.is_empty() {
        return Err(CallToolError::from_message("text is required"));
    }
    if params.text.len() > 4096 {
        return Err(CallToolError::from_message(
            "message exceeds 4096 byte limit",
        ));
    }

    state
        .game_sessions
        .send_message(wallet_pubkey, &params.text)
        .await
        .map_err(|e| CallToolError::from_message(format!("send_message failed: {e}")))?;

    let response = serde_json::json!({ "sent": true });
    Ok(CallToolResult::text_content(vec![TextContent::from(
        serde_json::to_string_pretty(&response)
            .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?,
    )]))
}

async fn handle_game_get_messages(
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    let messages = state
        .game_sessions
        .get_messages(wallet_pubkey)
        .await
        .map_err(|e| CallToolError::from_message(format!("get_messages failed: {e}")))?;

    let response = serde_json::json!({
        "messages": messages.iter().map(|m| serde_json::json!({ "text": m })).collect::<Vec<_>>(),
        "count": messages.len(),
    });

    Ok(CallToolResult::text_content(vec![TextContent::from(
        serde_json::to_string_pretty(&response)
            .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?,
    )]))
}

async fn handle_game_submit_guess(
    params: GameSubmitGuess,
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    let guess: u8 = match params.guess.to_lowercase().as_str() {
        "same" => 0,
        "different" => 1,
        _ => {
            return Err(CallToolError::from_message(
                "guess must be 'same' or 'different'",
            ))
        }
    };

    let outcome = state
        .game_sessions
        .submit_guess(wallet_pubkey, guess)
        .await
        .map_err(|e| CallToolError::from_message(format!("submit_guess failed: {e}")))?;

    let json = serde_json::to_string_pretty(&outcome)
        .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?;

    Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
}

async fn handle_game_get_result(
    state: &ToolState,
    wallet_pubkey: &str,
) -> Result<CallToolResult, CallToolError> {
    let result = state
        .game_sessions
        .get_result(wallet_pubkey)
        .await
        .map_err(|e| CallToolError::from_message(format!("get_result failed: {e}")))?;

    let json = serde_json::to_string_pretty(&result)
        .map_err(|e| CallToolError::from_message(format!("serialization failed: {e}")))?;

    Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
}

/// Parameters shared by claim_task and submit_work transaction construction.
struct TxConstructionParams<'a> {
    task_id: &'a str,
    client_pubkey: &'a str,
    wallet_pubkey: &'a str,
    session_keypair_bytes: &'a [u8],
    solana_rpc_url: &'a str,
    program_id: &'a str,
    rpc_client: &'a reqwest::Client,
}

/// Construct a claim_task Solana transaction, sign with session key, submit to RPC.
///
/// In production, this builds the actual Anchor instruction with the program IDL.
/// For now, it constructs a minimal transaction skeleton that demonstrates the flow.
async fn construct_and_submit_claim_task(
    params: &TxConstructionParams<'_>,
) -> Result<String, McpServiceError> {
    let task_id = params.task_id;
    let session_keypair_bytes = params.session_keypair_bytes;
    assert!(!task_id.is_empty(), "task_id must not be empty");
    assert!(
        session_keypair_bytes.len() == 64,
        "keypair must be 64 bytes"
    );

    let keypair = solana_sdk::signer::keypair::Keypair::try_from(session_keypair_bytes)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid session keypair: {e}")))?;

    let program_pubkey = solana_sdk::pubkey::Pubkey::from_str(params.program_id)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid program id: {e}")))?;

    let wallet_key = solana_sdk::pubkey::Pubkey::from_str(params.wallet_pubkey)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid wallet: {e}")))?;

    let client_key = solana_sdk::pubkey::Pubkey::from_str(params.client_pubkey)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid client pubkey: {e}")))?;

    let task_counter: u64 = task_id.parse().map_err(|e| {
        McpServiceError::TransactionError(format!("task_id must be a u64 task_counter: {e}"))
    })?;

    // Build instruction data: Anchor discriminator (8 bytes) + Borsh-serialized task_id
    let data = build_anchor_instruction_data("claim_task", &[task_id]);

    // Derive the SessionDelegate PDA: seeds = [b"session", agent_wallet, session_pubkey]
    let session_pubkey = keypair.pubkey();
    let (session_delegate_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"session", wallet_key.as_ref(), session_pubkey.as_ref()],
        &program_pubkey,
    );

    // Derive the Task PDA: seeds = [b"task", task_counter (u64 LE), client_pubkey]
    let (task_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"task", &task_counter.to_le_bytes(), client_key.as_ref()],
        &program_pubkey,
    );

    let instruction = solana_sdk::instruction::Instruction {
        program_id: program_pubkey,
        accounts: vec![
            solana_sdk::instruction::AccountMeta::new(task_pda, false),
            solana_sdk::instruction::AccountMeta::new_readonly(wallet_key, false),
            solana_sdk::instruction::AccountMeta::new_readonly(session_pubkey, true),
            solana_sdk::instruction::AccountMeta::new_readonly(session_delegate_pda, false),
        ],
        data,
    };

    assert!(
        instruction.accounts.len() == 4,
        "claim_task instruction must have exactly 4 accounts"
    );

    submit_transaction(
        params.rpc_client,
        &keypair,
        &[instruction],
        params.solana_rpc_url,
    )
    .await
}

/// Construct a submit_work Solana transaction, sign with session key, submit to RPC.
async fn construct_and_submit_work(
    tx_params: &TxConstructionParams<'_>,
    content_id: &str,
) -> Result<String, McpServiceError> {
    let task_id = tx_params.task_id;
    let session_keypair_bytes = tx_params.session_keypair_bytes;
    assert!(!task_id.is_empty(), "task_id must not be empty");
    assert!(!content_id.is_empty(), "content_id must not be empty");
    assert!(
        session_keypair_bytes.len() == 64,
        "keypair must be 64 bytes"
    );

    let keypair = solana_sdk::signer::keypair::Keypair::try_from(session_keypair_bytes)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid session keypair: {e}")))?;

    let program_pubkey = solana_sdk::pubkey::Pubkey::from_str(tx_params.program_id)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid program id: {e}")))?;

    let wallet_key = solana_sdk::pubkey::Pubkey::from_str(tx_params.wallet_pubkey)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid wallet: {e}")))?;

    let client_key = solana_sdk::pubkey::Pubkey::from_str(tx_params.client_pubkey)
        .map_err(|e| McpServiceError::TransactionError(format!("invalid client pubkey: {e}")))?;

    let task_counter: u64 = task_id.parse().map_err(|e| {
        McpServiceError::TransactionError(format!("task_id must be a u64 task_counter: {e}"))
    })?;

    // Build instruction data: Anchor discriminator (8 bytes) + Borsh-serialized args
    let data = build_anchor_instruction_data("submit_work", &[task_id, content_id]);

    // Derive the SessionDelegate PDA
    let session_pubkey = keypair.pubkey();
    let (session_delegate_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"session", wallet_key.as_ref(), session_pubkey.as_ref()],
        &program_pubkey,
    );

    // Derive the Task PDA: seeds = [b"task", task_counter (u64 LE), client_pubkey]
    let (task_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"task", &task_counter.to_le_bytes(), client_key.as_ref()],
        &program_pubkey,
    );

    let instruction = solana_sdk::instruction::Instruction {
        program_id: program_pubkey,
        accounts: vec![
            solana_sdk::instruction::AccountMeta::new(task_pda, false),
            solana_sdk::instruction::AccountMeta::new_readonly(wallet_key, false),
            solana_sdk::instruction::AccountMeta::new_readonly(session_pubkey, true),
            solana_sdk::instruction::AccountMeta::new_readonly(session_delegate_pda, false),
        ],
        data,
    };

    assert!(
        instruction.accounts.len() == 4,
        "submit_work instruction must have exactly 4 accounts"
    );

    submit_transaction(
        tx_params.rpc_client,
        &keypair,
        &[instruction],
        tx_params.solana_rpc_url,
    )
    .await
}

/// Validates a content ID: YouTube video IDs (11 alphanumeric/dash/underscore chars)
/// or X/Twitter tweet IDs (numeric, up to 20 digits).
fn is_valid_content_id(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }

    // YouTube: exactly 11 chars, alphanumeric + dash + underscore
    let is_youtube = id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');

    // X/Twitter: 1-20 digits (tweet IDs are numeric snowflakes)
    let is_tweet = id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit());

    is_youtube || is_tweet
}

/// Submit a signed transaction to the Solana RPC endpoint.
async fn submit_transaction(
    client: &reqwest::Client,
    signer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[solana_sdk::instruction::Instruction],
    rpc_url: &str,
) -> Result<String, McpServiceError> {
    let blockhash = fetch_latest_blockhash(client, rpc_url).await?;

    let message = solana_sdk::message::Message::new(instructions, Some(&signer.pubkey()));
    let transaction = solana_sdk::transaction::Transaction::new(&[signer], message, blockhash);

    let encoded = serialize_transaction(&transaction)?;

    let signature = send_raw_transaction(client, &encoded, rpc_url).await?;

    assert!(
        !signature.is_empty(),
        "transaction signature must not be empty"
    );

    Ok(signature)
}

/// Fetch the latest blockhash from the Solana RPC endpoint.
async fn fetch_latest_blockhash(
    client: &reqwest::Client,
    rpc_url: &str,
) -> Result<solana_sdk::hash::Hash, McpServiceError> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestBlockhash",
        "params": [{"commitment": "finalized"}]
    });

    let rpc_start = std::time::Instant::now();
    let response = client
        .post(rpc_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(
                service = "coordination-mcp-server",
                error = %e,
                rpc_url = %rpc_url,
                "solana RPC getLatestBlockhash failed"
            );
            McpServiceError::SolanaRpcError(format!("blockhash request failed: {e}"))
        })?;
    let latency = rpc_start.elapsed();
    tracing::debug!(
        service = "coordination-mcp-server",
        latency_ms = latency.as_millis() as u64,
        "getLatestBlockhash completed"
    );

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("blockhash parse failed: {e}")))?;

    let blockhash_str = json["result"]["value"]["blockhash"]
        .as_str()
        .ok_or_else(|| {
            McpServiceError::SolanaRpcError("missing blockhash in response".to_string())
        })?;

    blockhash_str
        .parse()
        .map_err(|e| McpServiceError::SolanaRpcError(format!("invalid blockhash: {e}")))
}

/// Serialize a Solana transaction to bs58-encoded bytes.
fn serialize_transaction(
    transaction: &solana_sdk::transaction::Transaction,
) -> Result<String, McpServiceError> {
    let serialized = bincode::serialize(transaction)
        .map_err(|e| McpServiceError::TransactionError(format!("serialization failed: {e}")))?;
    Ok(bs58::encode(&serialized).into_string())
}

/// Send a serialized transaction to the Solana RPC endpoint and return the signature.
async fn send_raw_transaction(
    client: &reqwest::Client,
    encoded_tx: &str,
    rpc_url: &str,
) -> Result<String, McpServiceError> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [encoded_tx, {"encoding": "base58"}]
    });

    let send_start = std::time::Instant::now();
    let response = client
        .post(rpc_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(
                service = "coordination-mcp-server",
                error = %e,
                rpc_url = %rpc_url,
                "solana RPC sendTransaction failed"
            );
            McpServiceError::SolanaRpcError(format!("send transaction failed: {e}"))
        })?;
    let latency = send_start.elapsed();
    tracing::debug!(
        service = "coordination-mcp-server",
        latency_ms = latency.as_millis() as u64,
        "sendTransaction completed"
    );

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("send response parse failed: {e}")))?;

    if let Some(error) = json.get("error") {
        return Err(McpServiceError::SolanaRpcError(format!(
            "transaction rejected: {error}"
        )));
    }

    json["result"]
        .as_str()
        .ok_or_else(|| McpServiceError::SolanaRpcError("missing signature in response".to_string()))
        .map(|s| s.to_string())
}

/// Build Anchor instruction data: discriminator (8 bytes) + Borsh-serialized string args.
/// Each string arg is serialized as a 4-byte little-endian length prefix + UTF-8 bytes.
fn build_anchor_instruction_data(instruction_name: &str, string_args: &[&str]) -> Vec<u8> {
    let discriminator = compute_anchor_discriminator(instruction_name);
    let args_size: usize = string_args
        .iter()
        .map(|s| s.len().saturating_add(4))
        .fold(0usize, |acc, x| acc.saturating_add(x));
    let total_size = 8usize.saturating_add(args_size);
    let mut data = Vec::with_capacity(total_size);
    data.extend_from_slice(&discriminator);
    for arg in string_args {
        // Borsh serializes strings as u32 length prefix (little-endian) + bytes
        let len = arg.len() as u32;
        data.extend_from_slice(&len.to_le_bytes());
        data.extend_from_slice(arg.as_bytes());
    }

    assert!(
        data.len() == total_size,
        "instruction data length must match expected size"
    );

    data
}

/// Compute the 8-byte Anchor instruction discriminator: SHA-256("global:<name>")[..8]
fn compute_anchor_discriminator(instruction_name: &str) -> [u8; 8] {
    use sha2::{Digest, Sha256};
    let input = format!("global:{instruction_name}");
    let hash = Sha256::digest(input.as_bytes());
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash[..8]);
    discriminator
}

fn to_call_tool_error(err: &McpServiceError) -> CallToolError {
    CallToolError::from_message(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_available() {
        let tools = CoordinationTools::tools();
        assert_eq!(tools.len(), 15);

        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        // Shillbot tools
        assert!(names.contains(&"list_available_tasks"));
        assert!(names.contains(&"get_task_details"));
        assert!(names.contains(&"claim_task"));
        assert!(names.contains(&"submit_work"));
        assert!(names.contains(&"check_earnings"));
        // Coordination Game tools
        assert!(names.contains(&"game_join_queue"));
        assert!(names.contains(&"game_get_leaderboard"));
        assert!(names.contains(&"game_info"));
        assert!(names.contains(&"game_register_wallet"));
        assert!(names.contains(&"game_find_match"));
        assert!(names.contains(&"game_check_match"));
        assert!(names.contains(&"game_send_message"));
        assert!(names.contains(&"game_get_messages"));
        assert!(names.contains(&"game_submit_guess"));
        assert!(names.contains(&"game_get_result"));
    }

    #[test]
    fn test_anchor_discriminator() {
        let disc1 = compute_anchor_discriminator("claim_task");
        let disc2 = compute_anchor_discriminator("submit_work");
        assert_ne!(disc1, disc2);
        assert_eq!(disc1.len(), 8);
    }

    #[test]
    fn test_list_tasks_tool_serialization() {
        let tool = ListAvailableTasks {
            limit: Some(10),
            min_price: Some(100_000),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let parsed: ListAvailableTasks = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.limit, Some(10));
        assert_eq!(parsed.min_price, Some(100_000));
    }

    #[test]
    fn test_claim_task_tool_serialization() {
        let tool = ClaimTask {
            task_id: "42".to_string(),
            client_pubkey: "11111111111111111111111111111111".to_string(),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let parsed: ClaimTask = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task_id, "42");
        assert_eq!(parsed.client_pubkey, "11111111111111111111111111111111");
    }

    #[test]
    fn test_submit_work_tool_serialization() {
        let tool = SubmitWork {
            task_id: "42".to_string(),
            content_id: "dQw4w9WgXcQ".to_string(),
            client_pubkey: "11111111111111111111111111111111".to_string(),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let parsed: SubmitWork = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task_id, "42");
        assert_eq!(parsed.content_id, "dQw4w9WgXcQ");
        assert_eq!(parsed.client_pubkey, "11111111111111111111111111111111");
    }

    #[test]
    fn test_check_earnings_tool_serialization() {
        let tool = CheckEarnings {};
        let json = serde_json::to_string(&tool).unwrap();
        let parsed: CheckEarnings = serde_json::from_str(&json).unwrap();
        let _ = parsed; // zero-field struct, just verify round-trip
    }

    #[test]
    fn test_anchor_discriminator_is_deterministic() {
        let disc1 = compute_anchor_discriminator("claim_task");
        let disc2 = compute_anchor_discriminator("claim_task");
        assert_eq!(disc1, disc2, "same input must produce same discriminator");
    }

    #[test]
    fn test_build_anchor_instruction_data_single_arg() {
        let data = build_anchor_instruction_data("claim_task", &["task_001"]);
        let disc = compute_anchor_discriminator("claim_task");

        // First 8 bytes are discriminator
        assert_eq!(&data[..8], &disc);

        // Next 4 bytes are string length (little-endian u32)
        let len_bytes: [u8; 4] = data[8..12].try_into().unwrap();
        let str_len = u32::from_le_bytes(len_bytes) as usize;
        assert_eq!(str_len, "task_001".len());

        // Remaining bytes are the string content
        let content = std::str::from_utf8(&data[12..12 + str_len]).unwrap();
        assert_eq!(content, "task_001");

        // Total length check
        assert_eq!(data.len(), 8 + 4 + "task_001".len());
    }

    #[test]
    fn test_build_anchor_instruction_data_two_args() {
        let data = build_anchor_instruction_data("submit_work", &["task_001", "dQw4w9WgXcQ"]);
        let disc = compute_anchor_discriminator("submit_work");

        assert_eq!(&data[..8], &disc);

        // First string: "task_001"
        let len1 = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        assert_eq!(len1, "task_001".len());
        assert_eq!(
            std::str::from_utf8(&data[12..12 + len1]).unwrap(),
            "task_001"
        );

        // Second string: "dQw4w9WgXcQ"
        let offset2 = 12 + len1;
        let len2 = u32::from_le_bytes(data[offset2..offset2 + 4].try_into().unwrap()) as usize;
        assert_eq!(len2, "dQw4w9WgXcQ".len());
        assert_eq!(
            std::str::from_utf8(&data[offset2 + 4..offset2 + 4 + len2]).unwrap(),
            "dQw4w9WgXcQ"
        );

        assert_eq!(data.len(), 8 + 4 + len1 + 4 + len2);
    }

    #[test]
    fn test_build_anchor_instruction_data_empty_args() {
        let data = build_anchor_instruction_data("some_ix", &[]);
        assert_eq!(data.len(), 8, "no args means only discriminator");
    }

    #[test]
    fn test_valid_content_ids() {
        assert!(is_valid_content_id("dQw4w9WgXcQ"));
        assert!(is_valid_content_id("abc-_12AB9z"));
        assert!(is_valid_content_id("2039199347657884078")); // tweet ID
        assert!(is_valid_content_id("1234567890"));
    }

    #[test]
    fn test_rejects_invalid_content_ids() {
        assert!(!is_valid_content_id(""));
        assert!(!is_valid_content_id("abc!@#$%^&*"));
        assert!(!is_valid_content_id("hello world"));
    }

    #[test]
    fn test_tool_name_mapping() {
        assert_eq!(
            tool_name(&CoordinationTools::ListAvailableTasks(ListAvailableTasks {
                limit: None,
                min_price: None,
            })),
            "list_available_tasks"
        );
        assert_eq!(
            tool_name(&CoordinationTools::ClaimTask(ClaimTask {
                task_id: String::new(),
                client_pubkey: String::new(),
            })),
            "claim_task"
        );
        assert_eq!(
            tool_name(&CoordinationTools::SubmitWork(SubmitWork {
                task_id: String::new(),
                content_id: String::new(),
                client_pubkey: String::new(),
            })),
            "submit_work"
        );
        assert_eq!(
            tool_name(&CoordinationTools::CheckEarnings(CheckEarnings {})),
            "check_earnings"
        );
    }
}
