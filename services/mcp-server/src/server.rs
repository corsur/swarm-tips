use crate::errors::McpServiceError;
use crate::game_proxy::GameApiProxy;
use crate::game_session::GameSessionManager;
use crate::listings::spending::{get_spending_opportunities, SpendingOpportunity};
use crate::listings::{get_listings, ListingsState};
use crate::proxy::OrchestratorProxy;
use crate::session_binding::McpSessionBinding;
use crate::solana_tx;
use rmcp::handler::server::common::Extension;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::JsonSchema;
use std::sync::Arc;

/// Header name the streamable HTTP MCP transport uses to carry the per-session
/// identifier on every request after `initialize`. Lowercase per HTTP/2 norms.
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Pull the streamable HTTP session ID out of the request parts so the
/// session-binding lookup has something to key on. Returns `None` for
/// pre-initialize requests or any caller that omits the header.
fn session_id_from_parts(parts: Option<&http::request::Parts>) -> Option<String> {
    parts?
        .headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Shared state accessible to all MCP sessions.
pub struct SharedState {
    pub orchestrator: OrchestratorProxy,
    /// Reserved adapter for future game-api flows (auth_challenge,
    /// join_queue). No live tool currently consumes it.
    #[allow(dead_code)]
    pub game_api: GameApiProxy,
    /// Default RPC URL — selected at startup based on `SOLANA_NETWORK`.
    /// Used for any read path that doesn't accept a per-call network
    /// override (currently the on-chain reads in `agent_profile` and the
    /// game leaderboard).
    pub solana_rpc_url: String,
    /// Mainnet RPC URL. Used by `shillbot_submit_tx` and
    /// `shillbot_verify_task` when `network == None | Some("mainnet")`.
    pub solana_rpc_url_mainnet: String,
    /// Devnet RPC URL. Used by `shillbot_submit_tx` and
    /// `shillbot_verify_task` when `network == Some("devnet")`. Without
    /// the right URL the broadcast lands on a different cluster from
    /// the unsigned tx and the orchestrator's confirm step fails.
    pub solana_rpc_url_devnet: String,
    pub rpc_client: reqwest::Client,
    pub game_sessions: Arc<GameSessionManager>,
    pub session_binding: Arc<McpSessionBinding>,
    /// Aggregated bounty/listing pipeline. Powers the unified
    /// `list_earning_opportunities` MCP tool by reading from the same
    /// Firestore-cached `get_listings` flow that backs the
    /// `/internal/listings` HTTP endpoint.
    pub listings: Arc<ListingsState>,
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
    /// Solana network. `"mainnet"` (default) or `"devnet"`. Forwarded to
    /// the orchestrator which dispatches per-network state. Mismatched
    /// network = the on-chain accounts won't be found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GetTaskDetailsArgs {
    /// The unique task identifier.
    pub task_id: String,
    /// Solana network. `"mainnet"` (default) or `"devnet"`. Forwarded to
    /// the orchestrator which dispatches per-network state. Mismatched
    /// network = the on-chain accounts won't be found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ClaimTaskArgs {
    /// The unique task identifier (format: `<campaign_id>:<task_uuid>`) returned
    /// by `list_available_tasks`.
    pub task_id: String,
    /// Solana network. `"mainnet"` (default) or `"devnet"`. Forwarded to
    /// the orchestrator which dispatches per-network state. Mismatched
    /// network = the on-chain accounts won't be found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GetAttestationArgs {
    /// Orchestrator-private Firestore document id (format:
    /// `<campaign_id>:<task_uuid>`). Use this if you got the id from
    /// `list_available_tasks` or `shillbot_check_earnings`. First-party
    /// path. Pass exactly one of `task_id` or `task_pda`.
    #[serde(default)]
    pub task_id: Option<String>,
    /// On-chain Task PDA (base58, e.g. `2K6jHZ1ZLhA1ZtKUGEzkxMa7TC7Nm1sMPVgKwFE6voci`).
    /// The canonical AAS identifier — derivable from any third-party
    /// indexer of the public `TaskCreated` event. Use this if you don't
    /// have access to the orchestrator's Firestore. Pass exactly one of
    /// `task_id` or `task_pda`.
    #[serde(default)]
    pub task_pda: Option<String>,
    /// Solana network to read the on-chain account from. `"mainnet"`
    /// (default) or `"devnet"`. Defaults to mainnet — pass `"devnet"`
    /// only if the task you're attesting was created on a devnet
    /// orchestrator. The orchestrator routes to a different RPC based
    /// on this value; mismatched network = the on-chain account won't
    /// be found and the call returns 409.
    #[serde(default)]
    pub network: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct SubmitWorkArgs {
    /// The unique task identifier (format: `<campaign_id>:<task_uuid>`).
    pub task_id: String,
    /// The content ID of the completed work (YouTube video ID, tweet ID,
    /// game session ID, etc.).
    pub content_id: String,
    /// Solana network. `"mainnet"` (default) or `"devnet"`. Forwarded to
    /// the orchestrator which dispatches per-network state. Mismatched
    /// network = the on-chain accounts won't be found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ShillbotSubmitTxArgs {
    /// The task identifier the signed tx applies to.
    pub task_id: String,
    /// `"claim"` for a signed `claim_task` tx, `"submit"` for `submit_work`.
    pub action: String,
    /// Base64-encoded signed Solana transaction returned by `claim_task` /
    /// `submit_work` and signed locally by the agent's wallet.
    pub signed_transaction: String,
    /// Solana network. `"mainnet"` (default) or `"devnet"`. Selects the
    /// RPC endpoint the signed transaction is broadcast to AND the
    /// orchestrator's per-network confirmation route. Mismatched network
    /// = the broadcast lands on a different cluster than the unsigned tx
    /// was built for, and the orchestrator's confirm step will not find
    /// the corresponding on-chain account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
}

/// Argument struct for tools that just need the network discriminator
/// (currently `shillbot_list_pending_approval`). Lets us mirror the
/// validation pattern from the other tools without inventing an empty
/// struct.
#[derive(Debug, serde::Deserialize, JsonSchema, Default)]
pub struct NetworkOnlyArgs {
    /// Solana network. `"mainnet"` (default) or `"devnet"`. Forwarded to
    /// the orchestrator which dispatches per-network state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameGetLeaderboardArgs {
    /// Tournament ID to get leaderboard for. Defaults to 1 (the only active tournament; omit unless you know what you're doing).
    pub tournament_id: Option<u64>,
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
pub struct XchainFindMatchArgs {
    /// 0x eth address of your per-match secp256k1 session key. You generate
    /// and hold the private key locally; the server only sees the address.
    pub session_key: String,
    /// Tournament ID to join. Defaults to 1.
    pub tournament_id: Option<u64>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct XchainBuildCreateMatchArgs {
    /// The `match` payload object from xchain_find_match / xchain_match_status.
    #[serde(rename = "match")]
    pub match_payload: serde_json::Value,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct XchainBuildRefundArgs {
    /// The `match` payload object from xchain_find_match / xchain_match_status.
    #[serde(rename = "match")]
    pub match_payload: serde_json::Value,
    /// "timeout" (default) or "nocert".
    pub kind: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameFindMatchArgs {
    /// Tournament ID to join. Defaults to 1 (the only active tournament; omit unless you know what you're doing).
    pub tournament_id: Option<u64>,
    /// Solana network. `"mainnet"` (default) or `"devnet"`. Selects which
    /// RPC endpoint is used to read the tournament + game_counter PDAs and
    /// build the deposit_stake message. Mismatched network = the on-chain
    /// accounts the program expects won't be found.
    #[serde(default)]
    pub network: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GameSubmitTxArgs {
    /// Base64-encoded signed Solana transaction.
    pub signed_transaction: String,
    /// The action this transaction performs: "deposit_stake", "join_game", "commit_guess", "reveal_guess", "create_game".
    pub action: String,
    /// Solana network. `"mainnet"` (default) or `"devnet"`. Must match the
    /// network used to build the unsigned tx — broadcasting to the wrong
    /// cluster = `BlockhashNotFound` rejection.
    #[serde(default)]
    pub network: Option<String>,
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

// -- Unified opportunity discovery parameter structs --

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ListEarningOpportunitiesArgs {
    /// Filter by source platform (e.g., "shillbot", "bountycaster", "moltlaunch", "botbounty"). Omit for all sources.
    pub source: Option<String>,
    /// Filter by category (e.g., "code", "content", "agent-services"). Omit for all categories.
    pub category: Option<String>,
    /// Minimum reward in USD. Omit for no floor. Listings without a USD estimate are excluded when set.
    pub min_reward_usd: Option<f64>,
    /// Maximum results to return. Default 50, max 200.
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ListSpendingOpportunitiesArgs {
    /// Filter by category (e.g., "video", "inference", "compute"). Omit for all categories.
    pub category: Option<String>,
    /// Maximum cost in USD. Omit for no ceiling. Opportunities without a USD estimate are always included.
    pub max_cost_usd: Option<f64>,
    /// Maximum results to return. Default 50, max 200.
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct AgentProfileArgs {
    /// Wallet pubkey to look up. Omit to query the caller's
    /// currently-registered wallet.
    pub wallet: Option<String>,
    /// Coordination Game tournament to read PlayerProfile under.
    /// Defaults to 1 (the only active tournament). PlayerProfile is
    /// per-tournament, so a player who has never joined this
    /// tournament returns `null` for the game half of the profile.
    pub tournament_id: Option<u64>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct CreditWebScoreArgs {
    /// Agent wallet (base58). Omit to use your registered wallet.
    pub wallet: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema, Default)]
pub struct ListExtensionsArgs {
    /// Filter to extensions where this wallet is the extender (base58).
    pub extender: Option<String>,
    /// Filter to extensions where this wallet is the recipient (base58).
    pub recipient: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct AgentTrustScoreArgs {
    /// Wallet pubkey to score. Omit to query the caller's
    /// currently-registered wallet.
    pub wallet: Option<String>,
    /// Coordination Game tournament to read PlayerProfile under.
    /// Defaults to 1.
    pub tournament_id: Option<u64>,
    /// Optional curator-tier ascription (`"first-party"`, `"vetted"`,
    /// `"discovered"`) — pass when you've already looked the agent up
    /// in the Layer 3 directory and want to fold the tier into the
    /// composite. Omit to skip the curator signal.
    pub curator_tier: Option<String>,
    /// Optional Hyperspace AgentRank score in 0..1. Pass when
    /// available; the composite formula folds it in. AgentRank
    /// integration is queued; for now callers compute it externally.
    pub agent_rank: Option<f64>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct SearchMcpServersArgs {
    /// Free-text needle, matched case-insensitively against entry
    /// name, description, why-listed, and tags. Omit to skip text
    /// filtering (and rely on `category` / `tier` instead).
    pub query: Option<String>,
    /// Filter by category (substring, case-insensitive). E.g.,
    /// `"agent"` matches `agent-bounties`, `agent-tools-directory`,
    /// etc.
    pub category: Option<String>,
    /// Filter by vetting tier: `"first-party"`, `"vetted"`, or
    /// `"discovered"`. Unknown values return zero results.
    pub tier: Option<String>,
    /// Maximum results to return. Default 50, max 200.
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct DiscoverOpportunitiesArgs {
    /// Restrict to one vertical: "earn" (agent gets paid) or "spend" (agent pays for a service).
    /// Omit to search both. Anything other than "earn" / "spend" is rejected.
    pub intent: Option<String>,
    /// Filter by category (substring, case-insensitive). Omit for all categories.
    pub category: Option<String>,
    /// Free-text needle matched case-insensitively against title, description, and tags.
    /// Omit to skip keyword filtering.
    pub keyword: Option<String>,
    /// Maximum results to return. Default 50, max 200.
    pub limit: Option<u32>,
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
        name = "shillbot_list_available_tasks",
        description = "[READ] List open Shillbot marketplace tasks. Agents can browse content creation opportunities (YouTube Shorts, X posts, etc.) with on-chain escrow. Returns task IDs, briefs, payment amounts, and platforms. Shillbot-specific deep query with brief/blocklist/brand-voice details — for cross-source aggregated discovery use list_earning_opportunities instead. Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_list_available_tasks(
        &self,
        Parameters(args): Parameters<ListAvailableTasksArgs>,
    ) -> Result<CallToolResult, McpError> {
        let network = parse_network_arg(args.network.as_deref())?;
        let result = self
            .state
            .orchestrator
            .list_tasks(args.limit, args.min_price, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            task_count = result.tasks.len(),
            network = network.unwrap_or("mainnet"),
            "listed available tasks"
        );
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_get_task_details",
        description = "[READ] Get full details for a Shillbot task: brief, blocklist, brand voice, platform, payment amount, and deadline. Use this before calling shillbot_claim_task. Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_get_task_details(
        &self,
        Parameters(args): Parameters<GetTaskDetailsArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        let network = parse_network_arg(args.network.as_deref())?;

        let result = self
            .state
            .orchestrator
            .get_task_details(&args.task_id, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            task_id = %args.task_id,
            network = network.unwrap_or("mainnet"),
            "retrieved task details"
        );
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_claim_task",
        description = "[STATE] Claim a Shillbot task. Returns an unsigned base64 Solana transaction the agent must sign locally with its wallet, then submit via shillbot_submit_tx with action=\"claim\". Non-custodial — the MCP server never sees your private key. Requires a registered wallet (call register_wallet first). Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(destructive_hint = true)
    )]
    async fn shillbot_claim_task(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        let network = parse_network_arg(args.network.as_deref())?;

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .claim_task(&args.task_id, &wallet_pubkey, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            task_id = %args.task_id,
            wallet = %wallet_pubkey,
            network = network.unwrap_or("mainnet"),
            "claim_task: unsigned tx built"
        );

        let result = serde_json::json!({
            "action": "claim",
            "task_id": response.task_id,
            "unsigned_tx": response.transaction,
            "instructions": "Sign this base64 transaction with your Solana wallet, then call shillbot_submit_tx with action=\"claim\" to broadcast and confirm the claim with the orchestrator.",
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_submit_work",
        description = "[EARN: SOL] Submit completed work for a claimed Shillbot task. Provide the content_id (YouTube video ID, tweet ID, game session ID, etc.). Returns an unsigned base64 Solana transaction — sign locally and submit via shillbot_submit_tx with action=\"submit\". On-chain verification runs at T+7d via Switchboard oracle, then payment is released based on engagement metrics. Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(destructive_hint = true)
    )]
    async fn shillbot_submit_work(
        &self,
        Parameters(args): Parameters<SubmitWorkArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        if args.content_id.is_empty() {
            return Err(invalid_input("content_id is required"));
        }
        let network = parse_network_arg(args.network.as_deref())?;

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .submit_task(&args.task_id, &wallet_pubkey, &args.content_id, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            task_id = %args.task_id,
            content_id = %args.content_id,
            wallet = %wallet_pubkey,
            network = network.unwrap_or("mainnet"),
            "submit_work: unsigned tx built"
        );

        let result = serde_json::json!({
            "action": "submit",
            "task_id": response.task_id,
            "content_id": args.content_id,
            "unsigned_tx": response.transaction,
            "instructions": "Sign this base64 transaction with your Solana wallet, then call shillbot_submit_tx with action=\"submit\" to broadcast and confirm submission with the orchestrator.",
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_verify_task",
        description = "[EARN: SOL] Build an unsigned verify_task transaction bundled with a per-task Switchboard oracle feed update. The verifier must have scored the task first (wait for the verification delay — 5 minutes for game-play, 7 days for YouTube). Sign the returned transaction locally, then submit via shillbot_submit_tx with action=\"verify\". One transaction, one fee — the oracle crank and on-chain verification happen atomically. Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(destructive_hint = true)
    )]
    async fn shillbot_verify_task(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>, // reuse — just needs task_id (+ network)
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        let network = parse_network_arg(args.network.as_deref())?;

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        // Get verification data from orchestrator
        let vdata = self
            .state
            .orchestrator
            .get_verification_data(&args.task_id, &wallet_pubkey, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        let rpc_url = self.rpc_url_for_network(network);
        let unsigned_tx =
            run_build_verify_tx(&args.task_id, &wallet_pubkey, &vdata, rpc_url).await?;

        let result = serde_json::json!({
            "action": "verify",
            "task_id": vdata.task_id,
            "unsigned_tx": unsigned_tx,
            "instructions": "Sign this transaction with your Solana wallet, then call shillbot_submit_tx with action=\"verify\".",
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_finalize_task",
        description = "[EARN: SOL] Finalize a verified Shillbot task after the challenge window. Transfers payment from on-chain escrow to the agent's wallet, protocol fee to treasury, and closes the task account. Permissionless — anyone can call after the challenge deadline. Sign the returned transaction locally, then submit via shillbot_submit_tx with action=\"finalize\". Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(destructive_hint = true)
    )]
    async fn shillbot_finalize_task(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>, // reuse — just needs task_id (+ network)
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        let network = parse_network_arg(args.network.as_deref())?;

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .build_finalize(&args.task_id, &wallet_pubkey, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        let result = serde_json::json!({
            "action": "finalize",
            "task_id": response.task_id,
            "unsigned_tx": response.transaction,
            "instructions": "Sign this transaction with your Solana wallet, then call shillbot_submit_tx with action=\"finalize\". Payment will be transferred from escrow to the agent's wallet.",
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_approve_task",
        description = "[IN DEVELOPMENT] [STATE] (CLIENT-SIDE) Approve agent-submitted content for a Shillbot task you funded. Returns an unsigned base64 Solana transaction the campaign client signs locally with their wallet, then submits via shillbot_submit_tx with action=\"approve\". Only the original task client may call this — the on-chain instruction enforces the wallet match. The verification timeout is anchored on submitted_at, NOT approved_at, so approving and then never funding oracle verification still returns the escrow at T+verification_timeout (no freeze attack). Use shillbot_list_pending_approval to find tasks awaiting your review. Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(destructive_hint = true)
    )]
    async fn shillbot_approve_task(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        let network = parse_network_arg(args.network.as_deref())?;

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .approve_task(&args.task_id, &wallet_pubkey, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            task_id = %args.task_id,
            wallet = %wallet_pubkey,
            network = network.unwrap_or("mainnet"),
            "shillbot_approve_task: unsigned tx built"
        );

        let result = serde_json::json!({
            "action": "approve",
            "task_id": response.task_id,
            "unsigned_tx": response.transaction,
            "instructions": "Sign this base64 transaction with your Solana wallet (must be the campaign client wallet). Then call shillbot_submit_tx with action=\"approve\" to broadcast and confirm the approval with the orchestrator. Verification by the oracle proceeds automatically once approval lands on-chain.",
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_reject_task",
        description = "[IN DEVELOPMENT] [READ] (CLIENT-SIDE, v1 STUB) Reject agent-submitted content. v1 has no first-class reject_task instruction yet — the reject path is implicit: don't call shillbot_approve_task and the on-chain expire_task crank returns the full escrow to the campaign's client wallet at T+verification_timeout (~14 days from submission). The response includes `expires_at` (the ISO-8601 timestamp at which expire_task becomes callable) so a client agent can schedule a follow-up. A first-class reject_task instruction with reason capture is on the roadmap; once it ships, this tool will route through it instead. Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_reject_task(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        let network = parse_network_arg(args.network.as_deref())?;

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        // Confirm the task is in a state where rejection is even meaningful
        // (Submitted). Reject from any other state would be a no-op or
        // misleading.
        let task = self
            .state
            .orchestrator
            .get_task_details(&args.task_id, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        if task.state != "submitted" {
            return Err(invalid_input(&format!(
                "task is in state {:?}, not 'submitted' — rejection only meaningful for submitted tasks awaiting client review",
                task.state
            )));
        }

        let expires_at = compute_expire_task_deadline(task.submitted_at.as_deref());

        tracing::info!(
            task_id = %args.task_id,
            wallet = %wallet_pubkey,
            expires_at = ?expires_at,
            "shillbot_reject_task: v1 stub — no on-chain action, escrow returns at expire_task"
        );

        let result =
            build_reject_v1_stub_response(&args.task_id, task.submitted_at.as_deref(), expires_at);
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_list_pending_approval",
        description = "[IN DEVELOPMENT] [READ] (CLIENT-SIDE) List Shillbot tasks awaiting your client review across all of your campaigns. Each entry is a task in 'submitted' state — agent has submitted content, you haven't yet called shillbot_approve_task or shillbot_reject_task on it. Use this to populate a review queue / inbox. Requires a registered wallet (the calling wallet must be the campaign client). Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_list_pending_approval(
        &self,
        Parameters(args): Parameters<NetworkOnlyArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let network = parse_network_arg(args.network.as_deref())?;
        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .list_pending_approval(&wallet_pubkey, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            wallet = %wallet_pubkey,
            count = response.tasks.len(),
            network = network.unwrap_or("mainnet"),
            "shillbot_list_pending_approval: queue returned"
        );

        let result = serde_json::json!({
            "tasks": response.tasks,
            "count": response.tasks.len(),
            "next_step": "For each task, call shillbot_get_task_details and shillbot_approve_task / shillbot_reject_task as appropriate.",
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_submit_tx",
        description = "[STATE] Broadcast a signed Shillbot Solana transaction (claim, submit, approve, verify, or finalize) and notify the orchestrator the action landed. Returns the on-chain signature and the orchestrator's confirmation message. Pair with claim_task / submit_work / approve_task / verify_task / finalize_task — those return the unsigned tx, this submits the signed result. Optional `network`: 'mainnet' (default) or 'devnet'. Pass the SAME network token here that you passed to the corresponding build tool — broadcasting on a different cluster than the unsigned tx was built for produces an InvalidAccount-shaped error.",
        annotations(destructive_hint = true)
    )]
    async fn shillbot_submit_tx(
        &self,
        Parameters(args): Parameters<ShillbotSubmitTxArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        if args.signed_transaction.is_empty() {
            return Err(invalid_input("signed_transaction is required"));
        }
        let action = parse_confirm_action(&args.action)?;
        let network = parse_network_arg(args.network.as_deref())?;

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let tx_signature = self
            .broadcast_and_wait_for_confirmation(&args.signed_transaction, network)
            .await?;

        tracing::info!(
            task_id = %args.task_id,
            wallet = %wallet_pubkey,
            action = %args.action,
            sig = %tx_signature,
            network = network.unwrap_or("mainnet"),
            "shillbot_submit_tx: tx broadcast"
        );

        let confirm = self
            .state
            .orchestrator
            .confirm_task(
                &args.task_id,
                &wallet_pubkey,
                &tx_signature,
                action,
                network,
            )
            .await
            .map_err(|e| to_mcp_error(&e))?;

        let result = serde_json::json!({
            "tx_signature": tx_signature,
            "task_id": confirm.task_id,
            "action": confirm.action,
            "message": confirm.message,
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_get_attestation",
        description = "[IN DEVELOPMENT] [READ] Fetch a portable AAS v0 attestation for a Verified Shillbot task. Pass `task_pda` (on-chain Task PDA, base58 — canonical, derivable from public TaskCreated event) for third-party verification, or `task_id` (orchestrator Firestore doc id) for first-party callers. Exactly one is required. Optional `network`: 'mainnet' (default) or 'devnet'. Returns `{version, network, program_id, task_pda, task_id, agent, composite_score, score_max, verified_at, verification_hash, content_hash, content_id_hash, switchboard_feed, verifier_instructions}`. Re-read the named PDA to verify; MCP does not sign. Capture window: between verify_task and finalize_task — closed accounts return 409 (PERMANENTLY UNAVAILABLE).",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_get_attestation(
        &self,
        Parameters(args): Parameters<GetAttestationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let network = parse_network_arg(args.network.as_deref())?;
        let attestation = match (
            args.task_id.as_deref().filter(|s| !s.is_empty()),
            args.task_pda.as_deref().filter(|s| !s.is_empty()),
        ) {
            (Some(_), Some(_)) => {
                return Err(invalid_input(
                    "pass exactly one of task_id or task_pda, not both",
                ));
            }
            (None, None) => {
                return Err(invalid_input(
                    "either task_id (Firestore doc id) or task_pda (on-chain PDA) is required",
                ));
            }
            (Some(task_id), None) => self
                .state
                .orchestrator
                .get_attestation(task_id, network)
                .await
                .map_err(|e| to_mcp_error(&e))?,
            (None, Some(task_pda)) => self
                .state
                .orchestrator
                .get_attestation_by_pda(task_pda, network)
                .await
                .map_err(|e| to_mcp_error(&e))?,
        };

        tracing::info!(
            task_id_arg = ?args.task_id,
            task_pda_arg = ?args.task_pda,
            composite_score = attestation.composite_score,
            "shillbot_get_attestation: AAS v0 attestation returned"
        );

        Ok(text_result(&attestation))
    }

    #[tool(
        name = "shillbot_complete_task",
        description = "[IN DEVELOPMENT] [READ] Single-call \"what do I do next?\" wrapper that collapses the multi-step Shillbot task lifecycle into one ask-then-execute loop. Pass a task_id; the tool reads the current on-chain + Firestore state, figures out whether you're the AGENT (claimer) or CLIENT (campaign owner) for this task, and returns a structured `next_action` block with the exact next tool to call and its arguments. The lifecycle has unavoidable external waits (T+7d oracle window for YouTube, client review, challenge window) — this tool surfaces them as `wait` actions with a `not_before` timestamp instead of a tool call. Re-call after each step (or after the wait elapses). Returns `done` when the task is Finalized. Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_complete_task(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }
        let network = parse_network_arg(args.network.as_deref())?;

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let task = self
            .state
            .orchestrator
            .get_task_details(&args.task_id, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        // The orchestrator's TaskResponse exposes `agent` (the claimer's
        // wallet, null until claimed) and the campaign reference, but not
        // the campaign's client wallet directly on the task. The proxy's
        // `TaskSummary` doesn't carry the client either; we treat
        // role-disambiguation as best-effort. AGENT-role hint is
        // unambiguous (the wallet equals task.agent); CLIENT-role hint is
        // surfaced when state == Submitted (the only state where the
        // client's next action matters), and we let the client confirm
        // their role themselves.
        let role = match task.state.as_str() {
            "submitted" => "client_or_agent",
            _ => {
                // We don't have task.agent in the TaskSummary. Default to
                // "agent" — most callers are the claimer; CLIENT-role
                // callers in any non-Submitted state have nothing to do
                // anyway.
                "agent"
            }
        };

        // Compute the verification-timeout deadline from `submitted_at`,
        // mirroring the pattern in `shillbot_reject_task` so the
        // `submitted` branch below can surface a real ISO timestamp
        // instead of `not_before: null`. 14 days = 1_209_600 seconds —
        // matches the on-chain `DEFAULT_VERIFICATION_TIMEOUT_SECONDS`
        // constant in `programs/shillbot/src/lib.rs`. Per-task overrides
        // can shorten this; the orchestrator doesn't currently surface
        // them on TaskSummary, so we use the conservative-upper-bound
        // default and let the agent re-confirm via
        // `shillbot_get_task_details` if they need finer precision.
        let escrow_expires_at = compute_expire_task_deadline(task.submitted_at.as_deref());
        let escrow_expires_iso = escrow_expires_at
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        let next =
            next_action_for_task_state(task.state.as_str(), &args.task_id, &escrow_expires_iso);

        tracing::info!(
            task_id = %args.task_id,
            wallet = %wallet_pubkey,
            state = %task.state,
            role,
            "shillbot_complete_task: next-action hint returned"
        );

        let result = serde_json::json!({
            "task_id": args.task_id,
            "current_state": task.state,
            "role": role,
            "wallet": wallet_pubkey,
            "next": next,
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_check_earnings",
        description = "[READ] Check your Shillbot earnings summary: total earned, pending payments, claimed tasks, completed tasks. Requires a registered wallet (use register_wallet first). Optional `network`: 'mainnet' (default) or 'devnet'.",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_check_earnings(
        &self,
        Parameters(args): Parameters<NetworkOnlyArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let network = parse_network_arg(args.network.as_deref())?;
        let wallet_pubkey = self.resolve_wallet(Some(&parts)).await.ok_or_else(|| {
            invalid_input("authentication required: connect your Solana wallet first")
        })?;

        let result = self
            .state
            .orchestrator
            .get_earnings(&wallet_pubkey, network)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            wallet = %wallet_pubkey,
            network = network.unwrap_or("mainnet"),
            "checked earnings"
        );
        Ok(text_result(&result))
    }

    #[tool(
        name = "agent_profile",
        description = "[IN DEVELOPMENT] [READ] Trustless on-chain reputation lookup. Reads AgentState (Shillbot: total_completed, total_earned, total_score_sum, total_tasks_claimed, total_challenges_lost) and PlayerProfile (Coordination Game per-tournament: wins, total_games, score) directly from Solana via getAccountInfo — no orchestrator hop, no cache. Returns derived metrics (average_score, completion_rate, dispute_rate, win_rate); either PDA may be absent (carries `null`). Pass `wallet` to query an agent; omit for your registered wallet. `tournament_id` defaults to 1.",
        annotations(read_only_hint = true)
    )]
    async fn agent_profile(
        &self,
        Parameters(args): Parameters<AgentProfileArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let target_wallet = self
            .resolve_target_wallet(args.wallet.as_deref(), Some(&parts))
            .await?;
        let tournament_id = args.tournament_id.unwrap_or(1);

        let (agent_state, player_profile) = self
            .read_agent_and_player_profile(&target_wallet, tournament_id)
            .await?;

        let derived = compute_shillbot_derived(agent_state.as_ref());
        let game_derived = compute_game_derived(player_profile.as_ref());

        let now_iso = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        let result = serde_json::json!({
            "wallet": target_wallet,
            "tournament_id": tournament_id,
            "shillbot": {
                "raw": agent_state,
                "derived": derived,
            },
            "coordination_game": {
                "raw": player_profile,
                "derived": game_derived,
            },
            "last_updated": now_iso,
            "source": "on-chain (Solana getAccountInfo); orchestrator NOT consulted.",
        });

        tracing::info!(
            wallet = %target_wallet,
            tournament_id,
            shillbot_present = agent_state.is_some(),
            player_profile_present = player_profile.is_some(),
            "agent_profile served"
        );

        Ok(text_result(&result))
    }

    #[tool(
        name = "agent_trust_score",
        description = "[IN DEVELOPMENT] [READ] Composite trust score (0..1) combining Shillbot reputation, Coordination Game win rate (≥ 5 games), Layer 3 curator tier, and (optionally) Hyperspace AgentRank. Partial-data tolerant — every signal is optional, weights renormalize over the present ones, and the response carries `confidence` (0..=4, how many signals contributed). Reads on-chain via the same path as agent_profile (#29); pass `curator_tier` / `agent_rank` if you have them. Returns a `breakdown` (per-signal value + applied weight) so the score is auditable. EigenTrust (the global trust graph) is a separate task that will compose with this once it ships.",
        annotations(read_only_hint = true)
    )]
    async fn agent_trust_score(
        &self,
        Parameters(args): Parameters<AgentTrustScoreArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let target_wallet = self
            .resolve_target_wallet(args.wallet.as_deref(), Some(&parts))
            .await?;
        let tournament_id = args.tournament_id.unwrap_or(1);

        let (agent_state, player_profile) = self
            .read_agent_and_player_profile(&target_wallet, tournament_id)
            .await?;

        use crate::composite_trust::{compute_trust, TrustInputs};

        let shillbot_input = build_shillbot_trust_input(agent_state.as_ref());
        let game_input = build_game_trust_input(player_profile.as_ref());
        let curator = parse_curator_tier(args.curator_tier.as_deref())?;

        // credit_web (B2): on-demand extension-credit web-position, read from
        // the same cluster as the other on-chain reads (empty/None on clusters
        // where extension-registry isn't deployed).
        let credit_web = self.read_credit_web_input(&target_wallet).await;

        let inputs = TrustInputs {
            shillbot: shillbot_input,
            game: game_input,
            curator,
            agent_rank: args.agent_rank,
            credit_web,
        };
        let trust = compute_trust(&inputs);

        let result = build_trust_score_response(
            &target_wallet,
            tournament_id,
            &trust,
            agent_state.is_some(),
            player_profile.is_some(),
            curator.is_some(),
            args.agent_rank.is_some(),
        );

        tracing::info!(
            wallet = %target_wallet,
            tournament_id,
            trust_score = trust.score,
            confidence = trust.confidence,
            "agent_trust_score served"
        );

        Ok(text_result(&result))
    }

    // -- Extension-credit tools (B5) --

    #[tool(
        name = "query_agent_credit_web_score",
        description = "[READ] Extension-credit web-position for an agent (0..1) — its standing in the extension graph (mund-creanc-witer), computed via EigenTrust anchored to the trusted root and gated on >= 1 received extension. Returns { wallet, position, extensions_received, has_standing }. This is the same signal that feeds credit_web in agent_trust_score. Empty/0 on clusters where extension-registry isn't deployed.",
        annotations(read_only_hint = true)
    )]
    async fn query_agent_credit_web_score(
        &self,
        Parameters(args): Parameters<CreditWebScoreArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let target_wallet = self
            .resolve_target_wallet(args.wallet.as_deref(), Some(&parts))
            .await?;
        let cw = self.read_credit_web_input(&target_wallet).await;
        let (position, extensions_received) = match &cw {
            Some(c) => (c.position, c.extensions_count),
            None => (None, 0),
        };
        let result = serde_json::json!({
            "wallet": target_wallet,
            "position": position,
            "extensions_received": extensions_received,
            "has_standing": position.is_some() && extensions_received >= 1,
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "list_extensions",
        description = "[READ] List active extension-credit obligations (extender -> recipient vouches backed by a bonded SOL stake). Optionally filter by `extender` or `recipient` wallet (base58). Returns { extensions: [{ extender, recipient, bond_lamports }], count }. Empty on clusters where extension-registry isn't deployed.",
        annotations(read_only_hint = true)
    )]
    async fn list_extensions(
        &self,
        Parameters(args): Parameters<ListExtensionsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let extensions = crate::solana_reads::read_all_extensions(
            &self.state.rpc_client,
            &self.state.solana_rpc_url,
        )
        .await
        .map_err(|e| to_mcp_error(&e))?;
        let filtered: Vec<serde_json::Value> = extensions
            .into_iter()
            .filter(|e| args.extender.as_deref().is_none_or(|x| e.extender == x))
            .filter(|e| args.recipient.as_deref().is_none_or(|x| e.recipient == x))
            .map(|e| {
                serde_json::json!({
                    "extender": e.extender,
                    "recipient": e.recipient,
                    "bond_lamports": e.bond_lamports,
                })
            })
            .collect();
        let count = filtered.len();
        let result = serde_json::json!({ "extensions": filtered, "count": count });
        Ok(text_result(&result))
    }

    // -- Video generation tools --

    #[tool(
        name = "generate_video",
        description = "[SPEND: 5 USDC] Generate a short-form video from a prompt or URL. Costs 5 USDC (Base/Ethereum/Polygon/Solana via x402). First call without tx_signature returns `{status: \"payment_required\", instructions, payment_details: {chain, address, amount, memo}}` from the x402 v2 protocol — pay the indicated amount to that address on that chain, then call again with tx_signature set to the broadcast tx hash to trigger generation. Returns a session_id to poll with check_video_status. Tip: the generated video can be submitted to a Shillbot task via shillbot_submit_work to earn back more than the spend.",
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

    // -- Unified opportunity discovery tools --

    #[tool(
        name = "list_earning_opportunities",
        description = "[IN DEVELOPMENT] [READ] Aggregated list of earning opportunities across the swarm.tips ecosystem. Includes Shillbot tasks (claim via shillbot_claim_task — first-party deep integration with on-chain Solana escrow + Switchboard oracle attestation), plus external bounties from Bountycaster, Moltlaunch, and BotBounty (each entry's `source_url` is a direct off-platform redirect — agents claim through the source platform itself, swarm.tips does not mediate). Each entry includes source, title, description, category, tags, reward amount/token/chain/USD estimate, posted_at, and (for first-party sources only) a `claim_via` field naming the in-MCP tool to call. This is the universal entry point for earning discovery — prefer it over per-source listing tools when they exist.",
        annotations(read_only_hint = true)
    )]
    async fn list_earning_opportunities(
        &self,
        Parameters(args): Parameters<ListEarningOpportunitiesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut listings = get_listings(&self.state.listings)
            .await
            .map_err(|e| McpError::internal_error(format!("get_listings failed: {e}"), None))?;

        // Apply args filters in-process. The cached listings come unfiltered;
        // we filter per-call so different agents can apply different filters
        // against the same cache.
        if let Some(source_filter) = args.source.as_deref() {
            let needle = source_filter.to_lowercase();
            listings.retain(|l| l.source.to_lowercase() == needle);
        }
        if let Some(category_filter) = args.category.as_deref() {
            let needle = category_filter.to_lowercase();
            listings.retain(|l| l.category.to_lowercase() == needle);
        }
        if let Some(min_usd) = args.min_reward_usd {
            listings.retain(|l| l.reward_usd_estimate.map(|v| v >= min_usd).unwrap_or(false));
        }

        // Annotate first-party entries with their in-MCP claim path. Pure
        // routing decision based on `source` — no extra calls.
        for listing in listings.iter_mut() {
            if listing.source == "shillbot" {
                listing.claim_via = Some("shillbot_claim_task".to_string());
            }
        }

        let limit = args.limit.unwrap_or(50).min(200) as usize;
        listings.truncate(limit);

        tracing::info!(
            count = listings.len(),
            source_filter = args.source.as_deref().unwrap_or(""),
            "list_earning_opportunities served"
        );
        Ok(text_result(&listings))
    }

    #[tool(
        name = "list_spending_opportunities",
        description = "[IN DEVELOPMENT] [READ] Aggregated list of paid services swarm.tips agents can spend on. v1 covers first-party services (generate_video — 5 USDC for an AI-generated short-form video). External spend sources (Chutes inference at llm.chutes.ai/v1, x402-paywalled APIs, etc.) are deferred to follow-up integrations. Each entry includes title, description, source, category, cost_amount/token/chain, USD estimate, direct redirect URL, and (for first-party services) a `spend_via` field naming the in-MCP tool to call. Use this to discover where to spend; for first-party services use the named `spend_via` tool, for external services navigate to the URL.",
        annotations(read_only_hint = true)
    )]
    async fn list_spending_opportunities(
        &self,
        Parameters(args): Parameters<ListSpendingOpportunitiesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut opportunities: Vec<SpendingOpportunity> =
            get_spending_opportunities(&self.state.rpc_client).await;

        if let Some(category_filter) = args.category.as_deref() {
            let needle = category_filter.to_lowercase();
            opportunities.retain(|o| o.category.to_lowercase() == needle);
        }
        if let Some(max_usd) = args.max_cost_usd {
            // Keep entries without a USD estimate (None) since we can't compare them.
            opportunities.retain(|o| o.cost_usd_estimate.map(|v| v <= max_usd).unwrap_or(true));
        }

        let limit = args.limit.unwrap_or(50).min(200) as usize;
        opportunities.truncate(limit);

        tracing::info!(
            count = opportunities.len(),
            "list_spending_opportunities served"
        );
        Ok(text_result(&opportunities))
    }

    #[tool(
        name = "discover_opportunities",
        description = "[IN DEVELOPMENT] [READ] Unified search across earn + spend verticals. Wraps `list_earning_opportunities` and `list_spending_opportunities` behind a single intent/category/keyword filter. Each returned entry carries a `vertical` field (`earn` or `spend`) so the caller can route it to the correct claim path. Use this when you don't know whether you want to earn or spend yet, or when you want to keyword-search across both. For deep per-vertical control (source-filter on earn, max-cost on spend) use the per-vertical tools directly.",
        annotations(read_only_hint = true)
    )]
    async fn discover_opportunities(
        &self,
        Parameters(args): Parameters<DiscoverOpportunitiesArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Validate intent up front so the caller gets a clean rejection
        // for typos rather than a silent "search both" fallback.
        let intent = parse_discover_intent(args.intent.as_deref())?;

        let category_needle = args.category.as_deref().map(|s| s.to_lowercase());
        let keyword_needle = args.keyword.as_deref().map(|s| s.to_lowercase());

        // Each entry below is annotated with `vertical` so a single
        // result list can carry both kinds without losing routing
        // information. We use serde_json::Value to merge the two
        // schemas (`Listing` + `SpendingOpportunity`) into one
        // homogeneous list — the caller can pick fields on the
        // discriminator.
        let mut merged: Vec<serde_json::Value> = Vec::new();

        if intent.map(|i| i == "earn").unwrap_or(true) {
            let listings = get_listings(&self.state.listings)
                .await
                .map_err(|e| McpError::internal_error(format!("get_listings failed: {e}"), None))?;
            collect_earn_entries(
                listings,
                category_needle.as_deref(),
                keyword_needle.as_deref(),
                &mut merged,
            );
        }

        if intent.map(|i| i == "spend").unwrap_or(true) {
            let opportunities = get_spending_opportunities(&self.state.rpc_client).await;
            collect_spend_entries(
                opportunities,
                category_needle.as_deref(),
                keyword_needle.as_deref(),
                &mut merged,
            );
        }

        let limit = args.limit.unwrap_or(50).min(200) as usize;
        merged.truncate(limit);

        tracing::info!(
            count = merged.len(),
            intent = intent.unwrap_or("any"),
            category = args.category.as_deref().unwrap_or(""),
            keyword = args.keyword.as_deref().unwrap_or(""),
            "discover_opportunities served"
        );

        Ok(text_result(&merged))
    }

    #[tool(
        name = "search_mcp_servers",
        description = "[IN DEVELOPMENT] [READ] Search the Layer 3 curated directory of MCP servers and agent-work tools. The directory has 30 entries across three vetting tiers — `first-party` (operated by the swarm.tips DAO), `vetted` (third-party, we've used + verified), `discovered` (cataloged from public sources, not yet exercised). Filter by `query` (substring vs name/description/tags), `category` (substring), and `tier`. Results sort first-party → vetted → discovered. The same directory powers swarm.tips/discover; this tool exposes it programmatically. Use this when an agent needs to find an MCP server for a capability (DeFi, search, browser automation, etc.) instead of an opportunity (which `discover_opportunities` covers).",
        annotations(read_only_hint = true)
    )]
    async fn search_mcp_servers(
        &self,
        Parameters(args): Parameters<SearchMcpServersArgs>,
    ) -> Result<CallToolResult, McpError> {
        let limit = args.limit.unwrap_or(50).min(200) as usize;
        let filter = crate::layer3::Filter {
            query: args.query.as_deref().filter(|s| !s.is_empty()),
            category: args.category.as_deref().filter(|s| !s.is_empty()),
            tier: args.tier.as_deref().filter(|s| !s.is_empty()),
            limit: Some(limit),
        };
        let hits = crate::layer3::search(&filter);

        tracing::info!(
            count = hits.len(),
            query = args.query.as_deref().unwrap_or(""),
            category = args.category.as_deref().unwrap_or(""),
            tier = args.tier.as_deref().unwrap_or(""),
            "search_mcp_servers served"
        );

        // Wrap with the directory-level metadata so the caller can
        // tell at a glance how many entries the directory has and
        // what the tier definitions mean. Mirrors the `/discover`
        // page's legend.
        let total_in_directory = crate::layer3::entries().len();
        let result = serde_json::json!({
            "results": hits,
            "total_in_directory": total_in_directory,
            "returned": hits.len(),
            "tier_definitions": {
                "first-party": "Operated by the swarm.tips DAO or a vertical we own. Trust = same as the marketplace itself.",
                "vetted": "Independently maintained by a third party; we have used it, confirmed the install path works, and verified the published capabilities match runtime behavior. Trust = we recommend it.",
                "discovered": "Cataloged from public sources (MCP registry, awesome-mcp lists, GitHub). Not yet exercised end-to-end. Trust = read at your own risk; we surface it for discovery, not endorsement.",
            },
            "browse_url": "https://swarm.tips/discover",
        });

        Ok(text_result(&result))
    }

    // -- Coordination Game tools --

    #[tool(
        name = "game_get_leaderboard",
        description = "[READ] Get the tournament leaderboard for the Coordination Game. Shows top players ranked by score (wins^2 / total_games). Tournament ID defaults to 1 (the only active tournament; omit unless you know what you're doing).",
        annotations(read_only_hint = true)
    )]
    async fn game_get_leaderboard(
        &self,
        Parameters(args): Parameters<GameGetLeaderboardArgs>,
    ) -> Result<CallToolResult, McpError> {
        let tournament_id = args.tournament_id.unwrap_or(1);
        let limit = args.limit.unwrap_or(20).min(100) as usize;

        // PlayerProfile data lives entirely on-chain; we read the PDAs
        // directly via RPC instead of going through game-api.
        let mut entries = crate::solana_reads::read_all_player_profiles_for_tournament(
            &self.state.rpc_client,
            &self.state.solana_rpc_url,
            tournament_id,
        )
        .await
        .map_err(|e| to_mcp_error(&e))?;

        entries.truncate(limit);

        let result = serde_json::json!({
            "tournament_id": tournament_id,
            "entries": entries,
        });

        tracing::info!(
            tournament_id,
            entries = entries.len(),
            "retrieved game leaderboard (on-chain read)"
        );
        Ok(text_result(&result))
    }

    #[tool(
        name = "register_wallet",
        description = "[STATE] Register your wallet to use any swarm.tips tool that touches funds. Provide a Solana base58 public key (32 bytes) for same-chain Coordination Game + Shillbot tools, OR an EVM 0x address (40 hex) for the cross-chain game leg (testnet: Base Sepolia) — call xchain_supported_chains first to choose. Non-custodial: your private key never leaves your device. Solana returns address + SOL balance; EVM returns your CAIP-10 account (the server holds no EVM RPC client, so check your own balance). One Solana registration covers every same-chain product (game_find_match, game_commit_guess, shillbot_claim_task, ...). The Mcp-Session-Id → wallet binding is persisted to Firestore so a pod restart doesn't strand the agent mid-game."
    )]
    async fn register_wallet(
        &self,
        Parameters(args): Parameters<GameRegisterWalletArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.pubkey.is_empty() {
            return Err(invalid_input("pubkey is required"));
        }

        // EVM (0x) wallets register for the cross-chain game leg. Intercept
        // before the Solana path: validate, bind the CAIP-10 account to the
        // session, and return — no Solana GameTxBuilder, no balance read.
        if args.pubkey.starts_with("0x") {
            let account_id = crate::xchain::evm_account_id(&args.pubkey)
                .map_err(|e| invalid_input(&format!("invalid EVM address: {e}")))?;
            if let Some(session_id) = session_id_from_parts(Some(&parts)) {
                let _ = self
                    .state
                    .session_binding
                    .bind(&session_id, &account_id)
                    .await;
            }
            // Demand signal for the mainnet-EVM gate (decision.md §6): now that
            // testnet accepts EVM wallets, the signal graduated from the old
            // register_wallet_bounce rejection to this acceptance event.
            tracing::info!(
                event = "register_wallet_evm",
                account = %account_id,
                "EVM wallet registered for cross-chain game"
            );
            return Ok(text_result(&crate::xchain::evm_registration_response(
                &account_id,
            )));
        }

        let (wallet, balance) = self
            .state
            .game_sessions
            .register_wallet(&args.pubkey)
            .await
            .map_err(|e| McpError::internal_error(format!("registration failed: {e}"), None))?;

        // Persist the streamable HTTP session → wallet binding so a pod
        // restart doesn't strand the agent. The next tool call from the
        // same `Mcp-Session-Id` resolves the wallet via Firestore even if
        // the in-memory game session map was wiped by the restart.
        if let Some(session_id) = session_id_from_parts(Some(&parts)) {
            // Best-effort: a binding write failure is logged inside
            // McpSessionBinding::bind and the agent can simply re-call
            // register_wallet to retry.
            let _ = self.state.session_binding.bind(&session_id, &wallet).await;
        }

        tracing::info!(wallet = %wallet, balance, "game wallet registered");

        let response = serde_json::json!({
            "wallet": wallet,
            "balance_lamports": balance,
            "status": "registered",
        });
        Ok(text_result(&response))
    }

    #[tool(
        name = "xchain_supported_chains",
        description = "[READ] Discover the chains you can play a cross-chain Coordination Game match on. Returns every registered chain (Solana + EVM) with its CAIP-2 id, native coin, per-match stake (in base units), float-pool tranche clamp, claim window, and deployed game-contract address, plus a plain-language description of how a cross-chain match is staked and settled. Call this before register_wallet to decide which wallet (Solana base58 or EVM 0x) to register. Read-only — no wallet required. Testnet only today (Solana devnet ↔ Base Sepolia); mainnet routes are gated.",
        annotations(read_only_hint = true)
    )]
    async fn xchain_supported_chains(&self) -> Result<CallToolResult, McpError> {
        let response = crate::xchain::supported_chains_response();
        tracing::info!(
            event = "xchain_supported_chains",
            chains = response["chains"].as_array().map(|a| a.len()).unwrap_or(0),
            "served cross-chain discovery"
        );
        Ok(text_result(&response))
    }

    #[tool(
        name = "xchain_find_match",
        description = "[STATE] Join the cross-chain Coordination Game queue and get matched with a player on the opposite chain (Solana ↔ EVM). You first generate a per-match secp256k1 session key locally (the server never sees its private key) and pass its 0x address here; the operator co-signs the match certificate against it. Requires a registered wallet (register_wallet — Solana base58 or EVM 0x). Returns status 'waiting' (poll xchain_match_status) or 'matched' with the co-signed match payload: both legs' contracts, stakes, deadlines, and the operator signature you need to fund your leg and settle. tournament_id defaults to 1. Testnet only (Solana devnet ↔ Base Sepolia).",
        annotations(destructive_hint = true)
    )]
    async fn xchain_find_match(
        &self,
        Parameters(args): Parameters<XchainFindMatchArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.session_key.is_empty() {
            return Err(invalid_input("session_key (0x address) is required"));
        }
        let bound = self.require_bound_wallet(Some(&parts)).await?;
        let (chain, address) = crate::xchain::resolve_xchain_wallet(&bound).ok_or_else(|| {
            invalid_input("registered wallet is not a cross-chain wallet (Solana base58 or EVM 0x)")
        })?;
        let tournament_id = args.tournament_id.unwrap_or(1);

        let resp = self
            .state
            .game_api
            .xqueue_join(&address, &chain, &args.session_key, tournament_id)
            .await
            .map_err(|e| McpError::internal_error(format!("xqueue_join failed: {e}"), None))?;

        tracing::info!(
            event = "xchain_find_match",
            wallet = %address,
            chain = %chain,
            status = %resp.status,
            "cross-chain queue join"
        );
        Ok(text_result(&serde_json::json!({
            "status": resp.status,
            "match": resp.match_payload,
            "chain": chain,
            "wallet": address,
            "next": "If 'waiting', poll xchain_match_status. If 'matched', use the returned match payload to fund your leg (build + sign the createMatch / create_xmatch tx) and later sign the outcome certificate with your session key.",
        })))
    }

    #[tool(
        name = "xchain_match_status",
        description = "[READ] Poll for your cross-chain match. Returns 'waiting' if not yet paired, or 'matched' with the co-signed match payload once an opposite-chain opponent joined. Call after xchain_find_match returned 'waiting'. Requires a registered wallet.",
        annotations(read_only_hint = true)
    )]
    async fn xchain_match_status(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let bound = self.require_bound_wallet(Some(&parts)).await?;
        let (chain, address) = crate::xchain::resolve_xchain_wallet(&bound)
            .ok_or_else(|| invalid_input("registered wallet is not a cross-chain wallet"))?;

        let resp = self
            .state
            .game_api
            .xqueue_status(&address)
            .await
            .map_err(|e| McpError::internal_error(format!("xqueue_status failed: {e}"), None))?;

        Ok(text_result(&serde_json::json!({
            "status": resp.status,
            "match": resp.match_payload,
            "chain": chain,
            "wallet": address,
        })))
    }

    #[tool(
        name = "xchain_build_create_xmatch",
        description = "[SPEND: 0.05 SOL] Build the matchmaker-cosigned Solana create_xmatch transaction to fund your leg of a cross-chain match. Solana-leg players only (register a Solana base58 wallet). After xchain_find_match returns 'matched', call this; it returns { unsigned_tx (base64), blockhash, matchmaker_signature, match_id }: assemble the fully-signed tx (matchmaker sig + your wallet sig) and broadcast via game_submit_tx with action='create_xmatch'. The matchmaker only ever cosigns a tx the backend built for your real pending match — it never signs arbitrary input.",
        annotations(destructive_hint = true)
    )]
    async fn xchain_build_create_xmatch(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let bound = self.require_bound_wallet(Some(&parts)).await?;
        let (chain, address) = crate::xchain::resolve_xchain_wallet(&bound)
            .ok_or_else(|| invalid_input("registered wallet is not a cross-chain wallet"))?;
        if !chain.starts_with("solana:") {
            return Err(invalid_input(
                "xchain_build_create_xmatch is for the Solana leg; EVM players use xchain_build_create_match",
            ));
        }

        let resp = self
            .state
            .game_api
            .xqueue_build_sol_fund(&address)
            .await
            .map_err(|e| McpError::internal_error(format!("build_sol_fund failed: {e}"), None))?;

        Ok(text_result(&serde_json::json!({
            "action": resp.action,
            "unsigned_tx": resp.unsigned_tx,
            "blockhash": resp.blockhash,
            "matchmaker_signature": resp.matchmaker_signature,
            "match_id": resp.match_id,
            "instructions": "Sign this transaction with your Solana wallet (the matchmaker signature is already provided), then broadcast it via game_submit_tx with action='create_xmatch'.",
        })))
    }

    #[tool(
        name = "xchain_build_settle",
        description = "[STATE] Get the operator-cosigned OUTCOME of your cross-chain match, ready to settle. Call after gameplay (both players' co-signed checkpoints have been relayed via the gameplay path). The operator derives the outcome from the relayed transcript — it never signs an outcome you supply — and returns { match_id, match_live_digest, outcome_kind, step_count, p1_guess, p2_guess, first_committer, matchup_type, transcript_hash, outcome_digest, operator_outcome_signature (oc_sigs[2]), operator_match_live_signature (live_sigs[2]) }. Sign outcome_digest with your per-match session key to produce your leg's oc_sig; combine with the counterparty's session sig + the operator sigs to assemble the permissionless settle on both legs (Solana settle_xmatch via game_submit_tx action='settle_xmatch'; EVM settle via your wallet). An equivocated match is rejected here — use the contested claim path instead.",
        annotations(read_only_hint = true)
    )]
    async fn xchain_build_settle(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let bound = self.require_bound_wallet(Some(&parts)).await?;
        let (_chain, address) = crate::xchain::resolve_xchain_wallet(&bound)
            .ok_or_else(|| invalid_input("registered wallet is not a cross-chain wallet"))?;

        let outcome = self
            .state
            .game_api
            .xqueue_outcome_cosign(&address)
            .await
            .map_err(|e| McpError::internal_error(format!("outcome_cosign failed: {e}"), None))?;

        Ok(text_result(&serde_json::json!({
            "outcome": outcome,
            "instructions": "Sign `outcome_digest` with your per-match session key to produce your leg's outcome signature (oc_sigs for your seat). Assemble settle with [legA session, legB session, operator] sigs over the outcome digest plus the match-live signatures; submit the Solana settle_xmatch via game_submit_tx with action='settle_xmatch' (permissionless) and the EVM settle from your own wallet.",
        })))
    }

    #[tool(
        name = "xchain_build_refund_xmatch",
        description = "[STATE] Build the unsigned Solana refund transaction to reclaim your stake on the Solana leg of a cross-chain match. Pass the `match` payload (from xchain_find_match/status) and kind='timeout' (after the claim window) or kind='nocert' (a funded match that never locked/cosigned). Refund is permissionless — you pay only the network fee. Returns { unsigned_tx, blockhash, match_id }: sign with your Solana wallet and broadcast via game_submit_tx. Solana-leg only; EVM players use xchain_build_refund.",
        annotations(destructive_hint = true)
    )]
    async fn xchain_build_refund_xmatch(
        &self,
        Parameters(args): Parameters<XchainBuildRefundArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let bound = self.require_bound_wallet(Some(&parts)).await?;
        let (chain, address) = crate::xchain::resolve_xchain_wallet(&bound)
            .ok_or_else(|| invalid_input("registered wallet is not a cross-chain wallet"))?;
        if !chain.starts_with("solana:") {
            return Err(invalid_input(
                "xchain_build_refund_xmatch is for the Solana leg; EVM players use xchain_build_refund",
            ));
        }
        let match_id = args
            .match_payload
            .get("match_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| invalid_input("match payload missing match_id"))?;
        let kind = args.kind.as_deref().unwrap_or("timeout");

        let resp = self
            .state
            .game_api
            .xqueue_build_sol_refund(&address, match_id, kind)
            .await
            .map_err(|e| McpError::internal_error(format!("build_sol_refund failed: {e}"), None))?;
        Ok(text_result(&serde_json::json!({
            "refund": resp,
            "instructions": "Sign the unsigned_tx with your Solana wallet and broadcast via game_submit_tx.",
        })))
    }

    #[tool(
        name = "xchain_build_create_match",
        description = "[SPEND] Build the unsigned EVM createMatch transaction to fund your leg of a cross-chain match. Pass the `match` payload object returned by xchain_find_match / xchain_match_status (when status was 'matched'). Returns { to, data, value_wei, chain, fund_deadline, match_deadline }: an EIP-1559 call you sign and submit with your EVM wallet (fill gas/nonce/chainId locally). value_wei is your stake sent as native ETH. EVM-leg players only; the Solana leg uses the Solana create_xmatch path.",
        annotations(destructive_hint = true)
    )]
    async fn xchain_build_create_match(
        &self,
        Parameters(args): Parameters<XchainBuildCreateMatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let call = crate::xchain::build_evm_create_match_call(&args.match_payload)
            .map_err(|e| invalid_input(&format!("invalid match payload: {e}")))?;
        Ok(text_result(&call))
    }

    #[tool(
        name = "xchain_build_lock",
        description = "[STATE] Build the unsigned EVM permissionless lockTranche transaction to lock your leg's cross-chain payout tranche after both players have funded. Pass the `match` payload (from xchain_find_match/status). The operator's match-live signature carried in the payload authorizes the lock — no operator action needed — and the locked amount is your leg's tranche from the signed cert. Lock is permissionless: you submit and pay only gas. Returns {to, data, value_wei, chain, match_id} to sign with your EVM wallet and submit. Must land before settle (settle requires Locked status). EVM-leg only; the Solana leg locks via its own path.",
        annotations(destructive_hint = true)
    )]
    async fn xchain_build_lock(
        &self,
        Parameters(args): Parameters<XchainBuildCreateMatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let call = crate::xchain::build_evm_lock_call(&args.match_payload)
            .map_err(|e| invalid_input(&format!("invalid match payload: {e}")))?;
        Ok(text_result(&call))
    }

    #[tool(
        name = "xchain_build_lock_xmatch",
        description = "[STATE] Build the unsigned permissionless Solana lock_xtranche transaction to lock your Solana leg's cross-chain payout tranche after both players have funded. No args — resolves your bound wallet. The operator's match-live signature (stored from pairing) authorizes the lock — no operator action — and you are the permissionless cranker/fee payer. Returns {unsigned_tx, blockhash, match_id, action}: sign with your Solana wallet and broadcast via game_submit_tx with action 'lock_xtranche'. Must land before settle (settle requires Locked). Solana-leg only; EVM players use xchain_build_lock.",
        annotations(destructive_hint = true)
    )]
    async fn xchain_build_lock_xmatch(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let bound = self.require_bound_wallet(Some(&parts)).await?;
        let (chain, address) = crate::xchain::resolve_xchain_wallet(&bound)
            .ok_or_else(|| invalid_input("registered wallet is not a cross-chain wallet"))?;
        if !chain.starts_with("solana:") {
            return Err(invalid_input(
                "xchain_build_lock_xmatch is for the Solana leg; EVM players use xchain_build_lock",
            ));
        }
        let resp = self
            .state
            .game_api
            .xqueue_build_sol_lock(&address)
            .await
            .map_err(|e| McpError::internal_error(format!("build_sol_lock failed: {e}"), None))?;
        Ok(text_result(&serde_json::json!({
            "lock": resp,
            "instructions": "Sign the unsigned_tx with your Solana wallet and broadcast via game_submit_tx with action 'lock_xtranche'. Permissionless — you pay only the network fee.",
        })))
    }

    #[tool(
        name = "xchain_build_refund",
        description = "[STATE] Build the unsigned EVM refund transaction to reclaim your stake on the EVM leg of a cross-chain match. Pass the `match` payload (from xchain_find_match/status) and kind='timeout' (after the claim window closes) or kind='nocert' (a funded match that never locked/cosigned a certificate). Refund is permissionless — you pay only gas. Returns {to, data, value_wei, chain} to sign and submit with your EVM wallet. EVM-leg only.",
        annotations(destructive_hint = true)
    )]
    async fn xchain_build_refund(
        &self,
        Parameters(args): Parameters<XchainBuildRefundArgs>,
    ) -> Result<CallToolResult, McpError> {
        let kind = args.kind.as_deref().unwrap_or("timeout");
        let call = crate::xchain::build_evm_refund_call(&args.match_payload, kind)
            .map_err(|e| invalid_input(&format!("invalid refund request: {e}")))?;
        Ok(text_result(&call))
    }

    #[tool(
        name = "game_find_match",
        description = "[SPEND: 0.05 SOL] Build an unsigned deposit_stake transaction to join the matchmaking queue. Sign the returned transaction locally, then submit it via game_submit_tx. The 0.05 SOL ante is locked until the game resolves — winning recovers your ante plus opponent's; losing forfeits to the prize pool. Negative-sum on average after the treasury cut. Requires a registered wallet (call register_wallet first). Tournament ID defaults to 1 (the only active tournament; omit unless you know what you're doing).",
        annotations(destructive_hint = true)
    )]
    async fn game_find_match(
        &self,
        Parameters(args): Parameters<GameFindMatchArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet(Some(&parts)).await?;
        let tournament_id = args.tournament_id.unwrap_or(1);

        let unsigned = self
            .state
            .game_sessions
            .build_find_match_tx(&wallet, tournament_id, args.network.as_deref())
            .await
            .map_err(|e| McpError::internal_error(format!("find_match failed: {e}"), None))?;

        let response = serde_json::json!({
            "action": "deposit_stake",
            "unsigned_tx": unsigned.transaction_b64,
            "blockhash": unsigned.blockhash,
            "num_signers": unsigned.num_signers,
            "tournament_id": tournament_id,
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
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet(Some(&parts)).await?;

        tracing::info!(wallet = %wallet, action = %args.action, "game_submit_tx: received");

        let result = self
            .state
            .game_sessions
            .submit_signed_game_tx(
                &wallet,
                &args.signed_transaction,
                &args.action,
                args.network.as_deref(),
            )
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
    async fn game_check_match(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet(Some(&parts)).await?;

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
        description = "[STATE] Send a chat message to your anonymous opponent during the game. Keep messages casual and human-like. Implicitly scoped to the active game in your current MCP session — no game_id needed. Resolution: Mcp-Session-Id header → registered wallet → active game session."
    )]
    async fn game_send_message(
        &self,
        Parameters(args): Parameters<GameSendMessageArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.text.is_empty() {
            return Err(invalid_input("text is required"));
        }
        if args.text.len() > 4096 {
            return Err(invalid_input("message exceeds 4096 byte limit"));
        }

        let wallet = self.require_game_wallet(Some(&parts)).await?;

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
        description = "[READ] Get all chat messages received from your opponent since the last call. Messages are drained from the buffer, so each message is returned only once. Implicitly scoped to the active game in your current MCP session — no game_id needed. Resolution: Mcp-Session-Id header → registered wallet → active game session.",
        annotations(read_only_hint = true)
    )]
    async fn game_get_messages(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet(Some(&parts)).await?;

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
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let guess: u8 = match args.guess.to_lowercase().as_str() {
            "same" => 0,
            "different" => 1,
            _ => return Err(invalid_input("guess must be 'same' or 'different'")),
        };

        let wallet = self.require_game_wallet(Some(&parts)).await?;

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
        description = "[STATE] Check if both players have committed. Returns 'waiting' if the opponent hasn't committed yet (poll every 3-5 seconds). When ready, returns an unsigned reveal transaction — sign it and submit via game_submit_tx with action='reveal_guess'. The reveal resolves the game: correct guess recovers your ante plus opponent's; wrong guess forfeits your ante to the prize pool. The game is negative-sum after the treasury cut.",
        annotations(destructive_hint = true)
    )]
    async fn game_reveal_guess(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet(Some(&parts)).await?;

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
    async fn game_get_result(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let wallet = self.require_game_wallet(Some(&parts)).await?;

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
    /// Resolve the agent's wallet for the current MCP request.
    ///
    /// Resolution: per-session Firestore binding only.
    ///
    /// `mcp-session-id` header → wallet via the `mcp_http_sessions`
    /// collection written by `register_wallet`. On hit, re-hydrates the
    /// in-memory game session from `mcp_game_sessions/{wallet}` so a
    /// pod restart doesn't strand the agent mid-game.
    ///
    /// Returns None if no binding exists. Caller surfaces the standard
    /// "no game session: call register_wallet first" error.
    ///
    /// No "first wallet in the map" fallback — that pattern leaked wallets
    /// across MCP sessions sharing a pod. A fresh session must call
    /// register_wallet to bind, otherwise this returns None.
    async fn resolve_wallet(&self, parts: Option<&http::request::Parts>) -> Option<String> {
        let session_id = session_id_from_parts(parts)?;
        let wallet = self.state.session_binding.resolve(&session_id).await?;
        // Re-hydrate game session from Firestore only if the in-memory
        // map doesn't already have it. The heavy work (RPC balance
        // check + persisted session load) only fires on the first tool
        // call after a pod restart; steady-state tool calls just hit
        // the cheap `contains_key` check.
        if !self.state.game_sessions.is_registered(&wallet).await {
            if let Err(e) = self.state.game_sessions.register_wallet(&wallet).await {
                tracing::warn!(
                    wallet = %wallet,
                    session_id = %session_id,
                    error = %e,
                    "failed to re-hydrate game session after binding lookup"
                );
            }
        }
        Some(wallet)
    }

    /// Require a registered game wallet, returning an MCP error if none
    /// exists. Same resolution order as `resolve_wallet`.
    async fn require_game_wallet(
        &self,
        parts: Option<&http::request::Parts>,
    ) -> Result<String, McpError> {
        self.resolve_wallet(parts)
            .await
            .ok_or_else(|| invalid_input("no game session: call register_wallet first"))
    }

    /// Resolve the session-bound wallet without hydrating a Solana game
    /// session — the cross-chain tools work for EVM wallets too, where the
    /// Solana hydration in `resolve_wallet` would warn harmlessly on every
    /// call. Returns the bound wallet string (CAIP-10 for EVM, base58 for
    /// Solana) or an MCP error if the session isn't registered.
    async fn require_bound_wallet(
        &self,
        parts: Option<&http::request::Parts>,
    ) -> Result<String, McpError> {
        let session_id =
            session_id_from_parts(parts).ok_or_else(|| invalid_input("missing Mcp-Session-Id"))?;
        self.state
            .session_binding
            .resolve(&session_id)
            .await
            .ok_or_else(|| invalid_input("no wallet registered: call register_wallet first"))
    }

    /// Resolve the wallet to query for `agent_profile` / `agent_trust_score`.
    /// Prefers the explicit `wallet` arg; falls back to the registered wallet
    /// for the current MCP session. Returns a 400 if neither is available.
    async fn resolve_target_wallet(
        &self,
        explicit: Option<&str>,
        parts: Option<&http::request::Parts>,
    ) -> Result<String, McpError> {
        match explicit {
            Some(w) if !w.is_empty() => Ok(w.to_string()),
            _ => self.resolve_wallet(parts).await.ok_or_else(|| {
                invalid_input(
                    "wallet arg omitted AND no registered wallet — call register_wallet first or pass wallet explicitly",
                )
            }),
        }
    }

    /// Concurrently read AgentState (Shillbot) and PlayerProfile (Coordination
    /// Game) for a wallet. The two PDAs live in different programs so the reads
    /// fan out under `tokio::join!`. Either may return `None` if the PDA is
    /// uninitialized.
    async fn read_agent_and_player_profile(
        &self,
        target_wallet: &str,
        tournament_id: u64,
    ) -> Result<
        (
            Option<crate::solana_reads::AgentStateData>,
            Option<crate::solana_reads::PlayerProfileData>,
        ),
        McpError,
    > {
        // Precondition: the caller must already have validated the wallet form.
        debug_assert!(!target_wallet.is_empty(), "target_wallet must be non-empty");
        let (agent_state, player_profile) = tokio::join!(
            crate::solana_reads::read_agent_state(
                &self.state.rpc_client,
                &self.state.solana_rpc_url,
                target_wallet,
            ),
            crate::solana_reads::read_player_profile(
                &self.state.rpc_client,
                &self.state.solana_rpc_url,
                target_wallet,
                tournament_id,
            ),
        );
        let agent_state = agent_state.map_err(|e| to_mcp_error(&e))?;
        let player_profile = player_profile.map_err(|e| to_mcp_error(&e))?;
        // Postcondition: a successful read returns the typed Option, never panics.
        Ok((agent_state, player_profile))
    }

    /// Compute the `credit_web` trust input (B2) for `target_wallet`: read all
    /// active extensions from the same cluster, compute the web-position graph
    /// anchored to the root, and surface the agent's normalized position +
    /// received-extension count. Returns `None` (signal absent) on any RPC
    /// error, when no extensions exist, or when the agent has received none —
    /// never fails the whole trust-score call.
    async fn read_credit_web_input(
        &self,
        target_wallet: &str,
    ) -> Option<crate::composite_trust::CreditWebInput> {
        // Shared with the /internal/agent-reputation HTTP endpoint.
        let (position, extensions_count) = crate::web_position::agent_web_position(
            &self.state.rpc_client,
            &self.state.solana_rpc_url,
            target_wallet,
        )
        .await;
        if extensions_count == 0 {
            return None;
        }
        Some(crate::composite_trust::CreditWebInput {
            position,
            extensions_count,
        })
    }

    /// Resolve the RPC URL the broadcast and on-chain bundle-script paths
    /// should hit for a given `network` token. `None` and `Some("mainnet")`
    /// both map to `solana_rpc_url_mainnet`; `Some("devnet")` maps to the
    /// devnet URL. Mismatched cluster = the broadcast lands on the wrong
    /// network from the unsigned tx and the orchestrator's confirm step
    /// can't find the on-chain account.
    fn rpc_url_for_network(&self, network: Option<&str>) -> &str {
        // Precondition: caller validated network ∈ {None, Some("devnet")}
        // upstream via `parse_network_arg`. Anything else means we'd
        // silently fall through to mainnet, which would mask a real bug.
        debug_assert!(
            matches!(network, None | Some("devnet") | Some("mainnet")),
            "rpc_url_for_network expects a parsed token, got {network:?}"
        );
        let url = match network {
            Some("devnet") => self.state.solana_rpc_url_devnet.as_str(),
            _ => self.state.solana_rpc_url_mainnet.as_str(),
        };
        // Postcondition: the URL is non-empty so callers don't accidentally
        // POST to "" and get a confusing reqwest builder error.
        debug_assert!(!url.is_empty(), "resolved RPC URL must be non-empty");
        url
    }

    /// Broadcast a base64-encoded signed transaction, then wait until the
    /// orchestrator's RPC view sees it confirmed. Returns the tx signature on
    /// success. The wait avoids the "transaction not found" race in
    /// `shillbot-api::solana::verify_tx_confirmed`.
    async fn broadcast_and_wait_for_confirmation(
        &self,
        signed_b64: &str,
        network: Option<&str>,
    ) -> Result<String, McpError> {
        // Precondition: caller validated non-empty input at the handler boundary.
        debug_assert!(
            !signed_b64.is_empty(),
            "signed transaction must be non-empty at broadcast time"
        );
        let rpc_url = self.rpc_url_for_network(network);
        let tx_signature =
            solana_tx::broadcast_signed_b64(&self.state.rpc_client, rpc_url, signed_b64)
                .await
                .map_err(|e| to_mcp_error(&e))?;

        solana_tx::wait_for_signature_confirmed(&self.state.rpc_client, rpc_url, &tx_signature, 30)
            .await
            .map_err(|e| to_mcp_error(&e))?;
        // Postcondition: a non-empty signature is returned only when both
        // broadcast and confirmation succeeded.
        debug_assert!(
            !tx_signature.is_empty(),
            "broadcast returned empty signature"
        );
        Ok(tx_signature)
    }
}

// -- Constants --

const INSTRUCTIONS: &str = "\
Swarm Tips MCP server (mcp.swarm.tips). Aggregated agent activities across multiple platforms.

## Tool categories
This server exposes 31 tools across five categories. If your agent only cares about a subset, configure your MCP client's tool allowlist to load only the prefixes below — most clients (Claude Code, Cursor, Continue) support per-server allowlists. Filtering at the client saves context tokens on every initialize.

- **game** (10 tools, prefix `game_*` plus `register_wallet`): Coordination Game on Solana mainnet. `register_wallet`, `game_get_leaderboard`, `game_find_match`, `game_submit_tx`, `game_check_match`, `game_send_message`, `game_get_messages`, `game_commit_guess`, `game_reveal_guess`, `game_get_result`.
- **shillbot** (13 tools, prefix `shillbot_*`): content-creation marketplace. AGENT side (earn): `shillbot_list_available_tasks`, `shillbot_get_task_details`, `shillbot_claim_task`, `shillbot_submit_work`, `shillbot_verify_task`, `shillbot_finalize_task`, `shillbot_submit_tx`, `shillbot_check_earnings`. CLIENT side (review submitted work): `shillbot_list_pending_approval`, `shillbot_approve_task`, `shillbot_reject_task`. CROSS-CUTTING: `shillbot_get_attestation` (AAS v0 portable proof for Verified/Finalized tasks; agent or third-party can read), `shillbot_complete_task` (single-call \"what do I do next?\" guide that collapses the 6-step lifecycle into one ask-then-execute loop). Note: `shillbot_verify_task` and `shillbot_finalize_task` are required to complete the EARN lifecycle on-chain — leaving them out of an allowlist locks your agent out of getting paid.
- **video** (2 tools): paid short-form video generation. `generate_video`, `check_video_status`.
- **listings** (4 tools): aggregated discovery across all sources. `list_earning_opportunities`, `list_spending_opportunities`, `discover_opportunities` (unified search across earn + spend with intent / category / keyword filters), `search_mcp_servers` (Layer 3 curated MCP-server directory — 30 entries with tier-aware trust framing).
- **profile** (2 tools, cross-cutting): `agent_profile` reads on-chain reputation directly via Solana RPC (no orchestrator hop). Combines Shillbot AgentState (claim / completion / score / dispute counters) and Coordination Game PlayerProfile (wins / total_games / score) plus derived metrics (average_score, completion_rate, dispute_rate, win_rate). `agent_trust_score` consumes the same on-chain reads + optional curator-tier + optional Hyperspace AgentRank and returns a single composite 0..1 trust score with a confidence count and per-signal breakdown for transparency.

`register_wallet` doubles as the `game` entry point and is also required for any `shillbot_*` STATE tool. If you load `shillbot` you should also load `register_wallet`.

Naive MCP clients that don't support per-server allowlists load all 31 tools by default. The friction-budget reduction is opt-in by your client — if your client always loads every advertised tool, this section is informational only.

## Wallet registration
1. register_wallet — register your Solana wallet (required for any STATE/SPEND/EARN tool). One registration covers every product (Coordination Game + Shillbot). Non-custodial: only the public key is registered, the private key stays on the agent.

## Coordination Game (coordination.game) — live on mainnet, Solana
Anonymous 1v1 social deduction. Stake 0.05 SOL, chat with a stranger, guess if they're on your team. The matchmaker decides whether your opponent is human or AI; the matchup type is hidden from you. Negative-sum on average after the treasury cut.
All transactions are non-custodial: the server returns unsigned transactions, you sign locally.

Rules for agents:
- You will NOT be told the matchup type — deduce from conversation
- Max chat message: 4096 bytes
- Commit timeout: ~1 hour, Reveal timeout: ~2 hours

How to play (after register_wallet):
1. game_find_match — returns unsigned deposit_stake transaction (tournament_id defaults to 1)
2. game_submit_tx — submit any signed game transaction (deposit, join, commit, reveal)
3. game_check_match — poll until matched (every 2-3 seconds). Returns unsigned join_game tx when matched.
4. game_send_message / game_get_messages — chat with opponent (implicit session scoping)
5. game_commit_guess — returns unsigned commit transaction
6. game_reveal_guess — poll until both committed, then reveals and resolves
7. game_get_result — see outcome
8. game_get_leaderboard — tournament rankings (read-only)

## Shillbot (shillbot.org) — content-creation marketplace, mainnet
Two-sided market: AGENTS earn SOL by creating content for paying CLIENTS. The full earn lifecycle is escrow → claim → submit → CLIENT REVIEW → oracle verify → finalize. Client review sits between submit and verify — a brand client has a hard gate to reject off-brand or unsafe content before any payment can flow.

### Agent flow (earn SOL)
1. shillbot_list_available_tasks — browse open tasks (or use list_earning_opportunities for cross-source aggregation)
2. shillbot_get_task_details — read brief, blocklist, brand voice, payment, deadline
3. shillbot_claim_task → shillbot_submit_tx (action=\"claim\") — claim
4. shillbot_submit_work → shillbot_submit_tx (action=\"submit\") — submit content_id once content is published. **Then wait for the client to approve.**
5. shillbot_verify_task → shillbot_submit_tx (action=\"verify\") — bundles oracle crank + verify. **Only callable on Approved state.** If you call earlier, the orchestrator returns 409 \"expected 'approved' for verify\".
6. shillbot_finalize_task → shillbot_submit_tx (action=\"finalize\") — releases payment from escrow after challenge window
7. shillbot_check_earnings — read your earnings summary

### Client flow (review submitted work)
ONLY the original campaign client can call these tools — the orchestrator and the on-chain instruction both verify wallet ownership.
1. shillbot_list_pending_approval — list submitted-but-not-yet-approved tasks across all your campaigns
2. shillbot_get_task_details — review the brief and the agent's submitted content_id
3. shillbot_approve_task → shillbot_submit_tx (action=\"approve\") — approve. The verifier then proceeds with oracle attestation automatically.
4. shillbot_reject_task — v1 stub: returns guidance; the actual reject path is implicit (don't approve and the on-chain expire_task crank returns the full escrow at T+verification_timeout, ~14 days from submission)

The verification timeout is anchored on submitted_at, NOT approved_at — a client cannot freeze an agent's escrow indefinitely by approving and then never funding oracle verification. The escrow always returns or the agent is paid by T+verification_timeout.

## Universal opportunity discovery
Two MCP tools aggregate earning + spending opportunities across the swarm.tips ecosystem and external platforms. First-party entries include a `claim_via` / `spend_via` field naming the in-MCP tool to call; external entries include a direct `source_url` redirect that the agent acts on off-platform.
1. list_earning_opportunities — Shillbot tasks, BotBounty / Bountycaster / Moltlaunch bounties (read-only aggregated)
2. list_spending_opportunities — first-party paid services (generate_video) plus future external sources

## Video Generation (shillbot.org) — 5 USDC per video
Generate short-form videos from a prompt or URL. Pay with USDC on Base, Ethereum, Polygon, or Solana via x402.
1. generate_video — first call: get payment instructions. Second call with tx_signature: start generation
2. check_video_status — poll by session_id until video_url is returned

## Signing transactions
Every `*_submit_tx` tool takes a base64-encoded SIGNED Solana transaction. The unsigned `transaction_b64` returned by upstream tools (`shillbot_claim_task`, `shillbot_submit_work`, `game_find_match`, `game_check_match`, `game_commit_guess`, `game_reveal_guess`) is **standard Solana wire format** — every major Solana library parses it directly.

**TypeScript / JavaScript** (`@solana/web3.js`, the most common path):
```ts
import { Transaction, Keypair } from \"@solana/web3.js\";
const tx = Transaction.from(Buffer.from(unsignedB64, \"base64\"));
tx.partialSign(keypair);
const signedB64 = tx.serialize().toString(\"base64\");
```

**Python** (`solders`):
```python
from solders.transaction import Transaction
tx = Transaction.from_bytes(base64.b64decode(unsigned_b64))
tx.sign([keypair], tx.message.recent_blockhash)
signed_b64 = base64.b64encode(bytes(tx)).decode()
```

**Rust** (`solana-sdk`): the repo ships `swarm-tips-repo/services/mcp-server/examples/sign_tx.rs` as a reference for Rust-native agents. Run `cargo run --release -p mcp-server --example sign_tx -- <base64-unsigned-tx> [<cosign-pubkey>:<cosign-sig-b64>]`. It handles single-signer txs and the matchmaker cosign case.

### Multi-signer: `game_check_match` returning `action: \"create_game\"`
This is the only dual-signer flow today. The tool returns three fields together: `unsigned_tx`, `matchmaker_signature` (base64, 64 bytes), and `blockhash`. The matchmaker pre-signs the message; you inject its signature into the right slot before adding your own. **Never recompute the message** — that invalidates the matchmaker's signature.

```ts
const tx = Transaction.from(Buffer.from(unsignedB64, \"base64\"));
// Find the slot whose pubkey is NOT yours — that's the matchmaker.
const numSigners = tx.compileMessage().header.numRequiredSignatures;
const accountKeys = tx.compileMessage().accountKeys;
let mmIdx = -1;
for (let i = 0; i < numSigners; i++) {
  if (!accountKeys[i].equals(keypair.publicKey)) { mmIdx = i; break; }
}
tx.signatures[mmIdx] = {
  publicKey: accountKeys[mmIdx],
  signature: Buffer.from(matchmakerSigB64, \"base64\"),
};
tx.partialSign(keypair);
const signedB64 = tx.serialize().toString(\"base64\");
```

A first-party TypeScript SDK that wraps the whole MCP flow (register → claim → sign → submit) is on the roadmap. Until it ships, the snippets above are all you need.

More info: https://swarm.tips/developers";

// -- Error helpers --

/// Compute the `derived` block of `agent_profile`'s Shillbot section. Each
/// rate is `null` when the denominator is zero, to keep division-by-zero
/// noise out of the response.
fn compute_shillbot_derived(
    agent_state: Option<&crate::solana_reads::AgentStateData>,
) -> serde_json::Value {
    let s = match agent_state {
        None => return serde_json::json!({}),
        Some(s) => s,
    };
    // Precondition: completed-task count never exceeds claimed-task count
    // (each completion was once a claim). Asserting this guards against
    // future on-chain accounting changes that swap the two counters.
    debug_assert!(
        s.total_completed <= s.total_tasks_claimed,
        "total_completed must not exceed total_tasks_claimed"
    );
    let avg_score = if s.total_completed > 0 {
        Some((s.total_score_sum as f64) / (s.total_completed as f64))
    } else {
        None
    };
    let completion_rate = if s.total_tasks_claimed > 0 {
        Some((s.total_completed as f64) / (s.total_tasks_claimed as f64))
    } else {
        None
    };
    let dispute_rate = if s.total_completed > 0 {
        Some((s.total_challenges_lost as f64) / (s.total_completed as f64))
    } else {
        None
    };
    // Postcondition: every rate is either None or a finite ratio.
    debug_assert!(
        completion_rate.map(|v| v.is_finite()).unwrap_or(true),
        "completion_rate must be finite when present"
    );
    serde_json::json!({
        "average_score": avg_score,
        "completion_rate": completion_rate,
        "dispute_rate": dispute_rate,
    })
}

/// Compute the `derived` block of `agent_profile`'s Coordination Game section.
/// `win_rate` is `null` when the player has zero recorded games.
fn compute_game_derived(
    player_profile: Option<&crate::solana_reads::PlayerProfileData>,
) -> serde_json::Value {
    let p = match player_profile {
        None => return serde_json::json!({}),
        Some(p) => p,
    };
    // Precondition: a player cannot have won more games than they played.
    debug_assert!(p.wins <= p.total_games, "wins must not exceed total_games");
    let win_rate = if p.total_games > 0 {
        Some((p.wins as f64) / (p.total_games as f64))
    } else {
        None
    };
    debug_assert!(
        win_rate.map(|v| (0.0..=1.0).contains(&v)).unwrap_or(true),
        "win_rate must be in [0, 1] when present"
    );
    serde_json::json!({ "win_rate": win_rate })
}

/// Build the `ShillbotInput` half of `TrustInputs` from an on-chain AgentState.
/// Returns `None` if AgentState is absent (PDA uninitialized). Uses the same
/// zero-denominator-as-None convention as `compute_shillbot_derived`.
fn build_shillbot_trust_input(
    agent_state: Option<&crate::solana_reads::AgentStateData>,
) -> Option<crate::composite_trust::ShillbotInput> {
    let s = agent_state?;
    let avg = if s.total_completed > 0 {
        Some((s.total_score_sum as f64) / (s.total_completed as f64))
    } else {
        None
    };
    let completion = if s.total_tasks_claimed > 0 {
        Some((s.total_completed as f64) / (s.total_tasks_claimed as f64))
    } else {
        None
    };
    // Postcondition: ratios are finite when present.
    debug_assert!(
        avg.map(|v| v.is_finite()).unwrap_or(true),
        "avg must be finite when present"
    );
    debug_assert!(
        completion.map(|v| v.is_finite()).unwrap_or(true),
        "completion must be finite when present"
    );
    Some(crate::composite_trust::ShillbotInput {
        average_score: avg,
        // MAX_SCORE = 1_000_000 per shared::MAX_SCORE; hardcoded here to
        // avoid pulling that crate as a dep, mirroring the orchestrator's
        // same hardcode at the AAS attestation surface (#16). Drift risk:
        // if the on-chain MAX_SCORE ever changes (it hasn't since v0),
        // this constant updates in lockstep with the on-chain commit.
        score_max: 1_000_000,
        completion_rate: completion,
        total_completed: s.total_completed,
    })
}

/// Build the `GameInput` half of `TrustInputs` from an on-chain PlayerProfile.
/// Returns `None` if the player has no profile yet (PDA uninitialized).
fn build_game_trust_input(
    player_profile: Option<&crate::solana_reads::PlayerProfileData>,
) -> Option<crate::composite_trust::GameInput> {
    let p = player_profile?;
    let win_rate = if p.total_games > 0 {
        Some((p.wins as f64) / (p.total_games as f64))
    } else {
        None
    };
    debug_assert!(
        win_rate.map(|v| (0.0..=1.0).contains(&v)).unwrap_or(true),
        "win_rate must be in [0, 1] when present"
    );
    Some(crate::composite_trust::GameInput {
        win_rate,
        total_games: p.total_games,
    })
}

/// Format the JSON wire response for `agent_trust_score`. The shape is
/// versioned via `trust_score_version` — integrators should pin to a known
/// version and re-validate on bumps.
fn build_trust_score_response(
    target_wallet: &str,
    tournament_id: u64,
    trust: &crate::composite_trust::TrustScore,
    shillbot_present: bool,
    game_present: bool,
    curator_present: bool,
    agent_rank_present: bool,
) -> serde_json::Value {
    debug_assert!(!target_wallet.is_empty(), "target_wallet must be non-empty");
    debug_assert!(
        (0.0..=1.0).contains(&trust.score),
        "trust.score must be in [0, 1]"
    );
    let now_iso = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    serde_json::json!({
        "wallet": target_wallet,
        "tournament_id": tournament_id,
        // Wire-format-stable formula version. v0 weights are pre-
        // calibration heuristics — they will change once we have
        // ≥100 unique client-agent pairs (the EigenTrust threshold
        // from the spec). Integrators should pin behavior to a
        // specific version and re-validate on bumps. Filed in
        // execution-v5/tasks.md as D5.
        "trust_score_version": "v0",
        "trust_score": trust.score,
        "confidence": trust.confidence,
        "breakdown": trust.breakdown,
        "inputs_present": {
            "shillbot": shillbot_present,
            "game": game_present,
            "curator": curator_present,
            "agent_rank": agent_rank_present,
        },
        "last_updated": now_iso,
    })
}

/// Parse the `curator_tier` argument for `agent_trust_score`. Returns
/// `Ok(None)` for empty/omitted, `Ok(Some(tier))` for the three valid tokens,
/// or a 400-shaped MCP error for any other string.
fn parse_curator_tier(
    raw: Option<&str>,
) -> Result<Option<crate::composite_trust::CuratorTier>, McpError> {
    use crate::composite_trust::CuratorTier;
    let result = match raw {
        Some("first-party") => Some(CuratorTier::FirstParty),
        Some("vetted") => Some(CuratorTier::Vetted),
        Some("discovered") => Some(CuratorTier::Discovered),
        None | Some("") => None,
        Some(other) => {
            return Err(invalid_input(&format!(
                "curator_tier must be \"first-party\", \"vetted\", \"discovered\", or omitted; got {other:?}"
            )));
        }
    };
    Ok(result)
}

/// Resolve the build-verify-tx.ts script path. In Docker the script lives at
/// `~/scripts/`; locally it sits next to `Cargo.toml`. The `BUILD_VERIFY_SCRIPT`
/// env var lets tests / one-offs override the resolution.
fn resolve_build_verify_script_path() -> std::path::PathBuf {
    std::env::var("BUILD_VERIFY_SCRIPT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts")
                .join("build-verify-tx.ts")
        })
}

/// Spawn `tsx build-verify-tx.ts` with the verification-data flags and return
/// the unsigned transaction (base64) the script prints to stdout. Surfaces
/// spawn-failure, non-zero-exit, and empty-output as separate MCP errors so
/// the agent gets actionable diagnostics.
async fn run_build_verify_tx(
    task_id: &str,
    payer: &str,
    vdata: &crate::proxy::VerificationDataResponse,
    rpc_url: &str,
) -> Result<String, McpError> {
    // Preconditions: identifying inputs must be non-empty so the spawned
    // script doesn't fail mid-arg-parse with a confusing error.
    debug_assert!(!task_id.is_empty(), "task_id must be non-empty");
    debug_assert!(!payer.is_empty(), "payer must be non-empty");

    let script_path = resolve_build_verify_script_path();
    let script_dir = script_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let output = tokio::process::Command::new("tsx")
        .current_dir(script_dir)
        .arg(&script_path)
        .arg("--task-id")
        .arg(task_id)
        .arg("--payer")
        .arg(payer)
        .arg("--score")
        .arg(vdata.composite_score.to_string())
        .arg("--hash")
        .arg(&vdata.verification_hash)
        .arg("--task-pda")
        .arg(&vdata.task_pda)
        .arg("--feed")
        .arg(&vdata.switchboard_feed)
        .arg("--global-state")
        .arg(&vdata.global_state)
        .arg("--rpc")
        .arg(rpc_url)
        .output()
        .await
        .map_err(|e| {
            tracing::error!(service = "mcp-server", error = %e, "failed to spawn build-verify-tx.ts");
            McpError::internal_error(format!("failed to spawn build-verify-tx: {e}"), None)
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!(service = "mcp-server", stderr = %stderr, "build-verify-tx.ts failed");
        return Err(McpError::internal_error(
            format!("build-verify-tx failed: {stderr}"),
            None,
        ));
    }

    let unsigned_tx = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if unsigned_tx.is_empty() {
        return Err(McpError::internal_error(
            "build-verify-tx produced empty output".to_string(),
            None,
        ));
    }
    // Postcondition: the function only returns Ok on success+non-empty stdout.
    debug_assert!(
        !unsigned_tx.is_empty(),
        "unsigned_tx must be non-empty on Ok path"
    );
    Ok(unsigned_tx)
}

/// Parse the optional `network` argument exposed by every Shillbot tool.
/// Returns `Ok(None)` for omitted / empty / `"mainnet"` / `"mainnet-beta"`
/// (the orchestrator default) and `Ok(Some("devnet"))` for explicit devnet
/// targeting. Anything else is rejected with a 400-shaped MCP error so
/// typos don't silently route to the default network. Mirrors the
/// validation pattern in `shillbot_get_attestation`.
fn parse_network_arg(raw: Option<&str>) -> Result<Option<&'static str>, McpError> {
    // Precondition: raw is provided as-deref'd Option<&str>; the empty
    // string is treated identically to None so JSON Schemas that fall
    // back to "" don't trip the validator.
    let result = match raw {
        None | Some("") | Some("mainnet") | Some("mainnet-beta") => None,
        Some("devnet") => Some("devnet"),
        Some(other) => {
            return Err(invalid_input(&format!(
                "network must be 'mainnet' or 'devnet', got '{other}'"
            )));
        }
    };
    // Postcondition: a Some(_) result is exactly the canonical "devnet"
    // token; nothing else can leak through.
    debug_assert!(
        matches!(result, None | Some("devnet")),
        "network must be None or Some(\"devnet\")"
    );
    Ok(result)
}

/// Parse the action discriminator passed to `shillbot_submit_tx`. Returns a
/// typed `ConfirmAction` or a 400-shaped MCP error listing the valid values.
fn parse_confirm_action(action: &str) -> Result<crate::proxy::ConfirmAction, McpError> {
    // Precondition: callers strip empty strings at the boundary; we still
    // produce a clear error rather than silently mapping `""` to a default.
    debug_assert!(!action.is_empty(), "action must be non-empty at parse time");
    let result = match action {
        "claim" => crate::proxy::ConfirmAction::Claim,
        "submit" => crate::proxy::ConfirmAction::Submit,
        "approve" => crate::proxy::ConfirmAction::Approve,
        "verify" => crate::proxy::ConfirmAction::Verify,
        "finalize" => crate::proxy::ConfirmAction::Finalize,
        other => {
            return Err(invalid_input(&format!(
                "action must be \"claim\", \"submit\", \"approve\", \"verify\", or \"finalize\", got {other:?}"
            )));
        }
    };
    // Postcondition: parse succeeded only for the five allowed strings above.
    Ok(result)
}

/// Default on-chain `expire_task` window for the v1 reject stub. Matches the
/// Anchor program's `DEFAULT_VERIFICATION_TIMEOUT_SECONDS = 1_209_600` (14 days).
/// Per-task on-chain overrides can shorten this; the orchestrator doesn't
/// surface them today so we use the conservative upper bound.
const DEFAULT_VERIFICATION_TIMEOUT_SECS: i64 = 1_209_600;

/// Compute the wall-clock deadline at which `expire_task` becomes callable
/// for a Submitted-state task: `submitted_at + DEFAULT_VERIFICATION_TIMEOUT_SECS`.
/// Returns `None` if `submitted_at` is missing or unparseable.
fn compute_expire_task_deadline(
    submitted_at: Option<&str>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let raw = submitted_at?;
    // Precondition: caller passes a non-empty timestamp.
    debug_assert!(
        !raw.is_empty(),
        "submitted_at must be non-empty when present"
    );
    let parsed = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
    let result = parsed
        .with_timezone(&chrono::Utc)
        .checked_add_signed(chrono::Duration::seconds(DEFAULT_VERIFICATION_TIMEOUT_SECS));
    // Postcondition: if we returned Some(t), t is strictly after the parsed input.
    debug_assert!(
        result
            .map(|t| t > parsed.with_timezone(&chrono::Utc))
            .unwrap_or(true),
        "computed deadline must follow submitted_at"
    );
    result
}

/// Build the v1 reject stub response payload for `shillbot_reject_task`.
/// Pure formatting — kept separate so the handler stays focused on auth +
/// state validation.
fn build_reject_v1_stub_response(
    task_id: &str,
    submitted_at: Option<&str>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> serde_json::Value {
    // Preconditions: identifying input must be non-empty and the timeout
    // constant must be the canonical value (mirrors the on-chain default).
    debug_assert!(!task_id.is_empty(), "task_id must be non-empty");
    debug_assert_eq!(
        DEFAULT_VERIFICATION_TIMEOUT_SECS, 1_209_600,
        "DEFAULT_VERIFICATION_TIMEOUT_SECS must match on-chain default"
    );
    serde_json::json!({
        "action": "reject_v1_stub",
        "task_id": task_id,
        "on_chain_action": "none",
        "submitted_at": submitted_at,
        "expires_at": expires_at.map(|dt| dt.to_rfc3339()),
        "verification_timeout_secs": DEFAULT_VERIFICATION_TIMEOUT_SECS,
        "guidance": "v1 reject is implicit: do NOT call shillbot_approve_task. The agent's submitted content stays in 'submitted' state. At T+verification_timeout (~14 days from the agent's submitted_at), expire_task can be cranked permissionlessly by anyone (including the campaign client) and the full escrow returns to the campaign's client wallet. The agent is paid nothing.",
        "next_step": "Wait until expires_at, then call expire_task (out-of-band crank — no MCP tool today; use solana CLI or the orchestrator's expire endpoint when available). Use the expires_at timestamp to schedule a follow-up reminder.",
        "future_work": "A first-class reject_task on-chain instruction with reason capture is on the roadmap. When it ships, this tool will route through it and shorten the rejection window.",
    })
}

/// Build the "next action" hint block for `shillbot_complete_task` based on
/// the task's current state. Extracted so the handler stays under the 60-line
/// rule and so the per-state logic is easy to scan.
fn next_action_for_task_state(
    state: &str,
    task_id: &str,
    escrow_expires_iso: &str,
) -> serde_json::Value {
    match state {
        "open" => serde_json::json!({
            "next_action": "tool_call",
            "next_tool": "shillbot_claim_task",
            "args": { "task_id": task_id },
            "hint": "Task is unclaimed. Call shillbot_claim_task to claim it; then sign the returned tx and submit via shillbot_submit_tx with action=\"claim\". Then call shillbot_complete_task again.",
        }),
        "claimed" => serde_json::json!({
            "next_action": "tool_call",
            "next_tool": "shillbot_submit_work",
            "args": { "task_id": task_id, "content_id": "<your published content id>" },
            "hint": "Task is claimed. Produce content per the task brief (call shillbot_get_task_details to re-read it), publish to the platform, then submit the content_id via shillbot_submit_work + shillbot_submit_tx with action=\"submit\". Then call shillbot_complete_task again.",
        }),
        "submitted" => {
            let timeout_str = if escrow_expires_iso.is_empty() {
                "submitted_at + ~14 days".to_string()
            } else {
                escrow_expires_iso.to_string()
            };
            serde_json::json!({
                "next_action": "wait",
                "wait_for": "client_review",
                "not_before": escrow_expires_iso,
                "hint": format!(
                    "Task is awaiting CLIENT review. If you are the campaign client, call shillbot_approve_task. If you are the agent, there is nothing for you to do until the client approves or the verification timeout returns the escrow at {}.",
                    timeout_str
                ),
                "client_actions": ["shillbot_approve_task", "shillbot_reject_task"],
                "agent_actions": [],
            })
        }
        "approved" => serde_json::json!({
            "next_action": "wait_or_call",
            "wait_for": "verifier_attestation",
            "next_tool": "shillbot_verify_task",
            "args": { "task_id": task_id },
            "hint": "Client approved. The off-chain verifier will produce the oracle attestation (5min for game-play tasks, 7d for YouTube). When the attestation is ready, call shillbot_verify_task + shillbot_submit_tx with action=\"verify\". Calling too early returns AttestationStale; back off and retry.",
        }),
        "verified" => serde_json::json!({
            "next_action": "wait_then_call",
            "wait_for": "challenge_window",
            "next_tool": "shillbot_finalize_task",
            "args": { "task_id": task_id },
            "hint": "Verified. The challenge window (24h default) must elapse before finalize. Once it has, call shillbot_finalize_task + shillbot_submit_tx with action=\"finalize\" to release the payment from escrow. Permissionless crank — anyone can finalize after the window.",
        }),
        "finalized" => serde_json::json!({
            "next_action": "done",
            "hint": "Payment has been released from escrow. Call shillbot_check_earnings to confirm. Optionally call shillbot_get_attestation BEFORE the on-chain account closes if you want a portable AAS attestation — note the capture window (spec docs/specs/aas-v1.md §6).",
        }),
        "disputed" => serde_json::json!({
            "next_action": "wait",
            "wait_for": "challenge_resolution",
            "hint": "Task is under dispute. The upgrade authority will call resolve_challenge. No agent / client action required.",
        }),
        "resolved" => serde_json::json!({
            "next_action": "done",
            "hint": "Challenge resolved. Funds have been distributed per the resolution. Call shillbot_check_earnings to see your share.",
        }),
        "expired" => serde_json::json!({
            "next_action": "done",
            "hint": "Task expired (verification timeout). Escrow has been returned to the client. No agent action available.",
        }),
        other => serde_json::json!({
            "next_action": "unknown",
            "hint": format!("Task is in unrecognized state {other:?}. Re-fetch via shillbot_get_task_details and inspect manually."),
        }),
    }
}

fn to_mcp_error(err: &McpServiceError) -> McpError {
    McpError::internal_error(err.to_string(), None)
}

fn invalid_input(msg: &str) -> McpError {
    McpError::invalid_params(msg.to_string(), None)
}

/// Parse the `intent` discriminator passed to `discover_opportunities`.
/// Returns `Ok(Some("earn"))`, `Ok(Some("spend"))`, or `Ok(None)` for the
/// "search both" omitted case. Rejects unknown values with a 400-shaped MCP
/// error so typos don't silently fall back to a wide scan.
fn parse_discover_intent(raw: Option<&str>) -> Result<Option<&'static str>, McpError> {
    let result = match raw {
        None | Some("") => None,
        Some("earn") => Some("earn"),
        Some("spend") => Some("spend"),
        Some(other) => {
            return Err(invalid_input(&format!(
                "intent must be \"earn\", \"spend\", or omitted; got {other:?}"
            )));
        }
    };
    // Postcondition: a Some(_) result is exactly one of the two valid tokens.
    debug_assert!(
        matches!(result, None | Some("earn") | Some("spend")),
        "intent must be earn/spend/None"
    );
    Ok(result)
}

/// Filter, annotate, and append earning listings to `merged`. Mutates each
/// listing in place to attach `claim_via` for first-party (`shillbot`) entries.
fn collect_earn_entries(
    listings: Vec<crate::listings::models::AgentJob>,
    category_needle: Option<&str>,
    keyword_needle: Option<&str>,
    merged: &mut Vec<serde_json::Value>,
) {
    let initial_len = merged.len();
    for mut listing in listings {
        if listing.source == "shillbot" {
            listing.claim_via = Some("shillbot_claim_task".to_string());
        }
        if !category_matches(&listing.category, category_needle) {
            continue;
        }
        if !keyword_matches_earn(&listing, keyword_needle) {
            continue;
        }
        let mut value = serde_json::to_value(&listing).unwrap_or(serde_json::Value::Null);
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "vertical".to_string(),
                serde_json::Value::String("earn".to_string()),
            );
        }
        merged.push(value);
    }
    // Postcondition: we never shrink the merged vector and never panic.
    debug_assert!(merged.len() >= initial_len, "merged must only grow");
}

/// Filter, annotate, and append spending opportunities to `merged`.
fn collect_spend_entries(
    opportunities: Vec<crate::listings::spending::SpendingOpportunity>,
    category_needle: Option<&str>,
    keyword_needle: Option<&str>,
    merged: &mut Vec<serde_json::Value>,
) {
    let initial_len = merged.len();
    for opp in opportunities {
        if !category_matches(&opp.category, category_needle) {
            continue;
        }
        if !keyword_matches_spend(&opp, keyword_needle) {
            continue;
        }
        let mut value = serde_json::to_value(&opp).unwrap_or(serde_json::Value::Null);
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "vertical".to_string(),
                serde_json::Value::String("spend".to_string()),
            );
        }
        merged.push(value);
    }
    // Postcondition: we never shrink the merged vector and never panic.
    debug_assert!(merged.len() >= initial_len, "merged must only grow");
}

/// `discover_opportunities` filter helper — entry's category contains
/// the lowercase `needle` substring. `None` needle means "no filter."
fn category_matches(category: &str, needle: Option<&str>) -> bool {
    match needle {
        None => true,
        Some(n) => category.to_lowercase().contains(n),
    }
}

/// `discover_opportunities` keyword filter for earn entries — match
/// against title, description, OR any tag (case-insensitive substring).
fn keyword_matches_earn(listing: &crate::listings::models::AgentJob, needle: Option<&str>) -> bool {
    let n = match needle {
        None => return true,
        Some(s) => s,
    };
    if listing.title.to_lowercase().contains(n) {
        return true;
    }
    if listing.description.to_lowercase().contains(n) {
        return true;
    }
    listing.tags.iter().any(|t| t.to_lowercase().contains(n))
}

/// `discover_opportunities` keyword filter for spend entries — same
/// shape as earn (no tags on SpendingOpportunity, so just title +
/// description).
fn keyword_matches_spend(
    opp: &crate::listings::spending::SpendingOpportunity,
    needle: Option<&str>,
) -> bool {
    let n = match needle {
        None => return true,
        Some(s) => s,
    };
    opp.title.to_lowercase().contains(n) || opp.description.to_lowercase().contains(n)
}

fn text_result(value: &impl serde::Serialize) -> CallToolResult {
    let json = serde_json::to_string_pretty(value)
        .unwrap_or_else(|e| format!("{{\"error\": \"serialization failed: {e}\"}}"));
    CallToolResult::success(vec![Content::text(json)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_network_arg_accepts_omitted_and_default_aliases() {
        // None, empty, mainnet, mainnet-beta all collapse to None so the
        // proxy emits no query suffix and the orchestrator routes to its
        // mainnet default. This is the behaviour every existing client
        // depends on; a regression here would silently break every
        // mainnet caller.
        assert!(parse_network_arg(None).expect("ok").is_none());
        assert!(parse_network_arg(Some("")).expect("ok").is_none());
        assert!(parse_network_arg(Some("mainnet")).expect("ok").is_none());
        assert!(parse_network_arg(Some("mainnet-beta"))
            .expect("ok")
            .is_none());
    }

    #[test]
    fn parse_network_arg_accepts_devnet() {
        // The only non-None value the helper is allowed to return —
        // anything else is a typo we want to reject loudly.
        assert_eq!(
            parse_network_arg(Some("devnet")).expect("ok"),
            Some("devnet")
        );
    }

    #[test]
    fn parse_network_arg_rejects_unknown_tokens() {
        // Typos like "stagenet" / "testnet" must be rejected so they
        // don't silently fall back to mainnet — that would route the
        // call to the wrong cluster and produce confusing
        // AccountNotFound errors downstream.
        let err =
            parse_network_arg(Some("stagenet")).expect_err("must reject unknown network token");
        let msg = format!("{err}");
        assert!(
            msg.contains("stagenet") && msg.contains("mainnet"),
            "error message should name the rejected token and the valid set, got: {msg}"
        );
    }
}
