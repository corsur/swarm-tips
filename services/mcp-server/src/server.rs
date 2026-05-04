use crate::auth::ChallengeManager;
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
    pub game_api: GameApiProxy,
    pub solana_rpc_url: String,
    pub rpc_client: reqwest::Client,
    pub game_sessions: Arc<GameSessionManager>,
    #[allow(dead_code)]
    pub challenge_manager: Arc<ChallengeManager>,
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
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct GetTaskDetailsArgs {
    /// The unique task identifier.
    pub task_id: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ClaimTaskArgs {
    /// The unique task identifier (format: `<campaign_id>:<task_uuid>`) returned
    /// by `list_available_tasks`.
    pub task_id: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct SubmitWorkArgs {
    /// The unique task identifier (format: `<campaign_id>:<task_uuid>`).
    pub task_id: String,
    /// The content ID of the completed work (YouTube video ID, tweet ID,
    /// game session ID, etc.).
    pub content_id: String,
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
pub struct GameFindMatchArgs {
    /// Tournament ID to join. Defaults to 1 (the only active tournament; omit unless you know what you're doing).
    pub tournament_id: Option<u64>,
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
        description = "[READ] List open Shillbot marketplace tasks. Agents can browse content creation opportunities (YouTube Shorts, X posts, etc.) with on-chain escrow. Returns task IDs, briefs, payment amounts, and platforms. Shillbot-specific deep query with brief/blocklist/brand-voice details — for cross-source aggregated discovery use list_earning_opportunities instead.",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_list_available_tasks(
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
        name = "shillbot_get_task_details",
        description = "[READ] Get full details for a Shillbot task: brief, blocklist, brand voice, platform, payment amount, and deadline. Use this before calling shillbot_claim_task.",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_get_task_details(
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
        name = "shillbot_claim_task",
        description = "[STATE] Claim a Shillbot task. Returns an unsigned base64 Solana transaction the agent must sign locally with its wallet, then submit via shillbot_submit_tx with action=\"claim\". Non-custodial — the MCP server never sees your private key. Requires a registered wallet (call register_wallet first).",
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

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .claim_task(&args.task_id, &wallet_pubkey)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(task_id = %args.task_id, wallet = %wallet_pubkey, "claim_task: unsigned tx built");

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
        description = "[EARN: SOL] Submit completed work for a claimed Shillbot task. Provide the content_id (YouTube video ID, tweet ID, game session ID, etc.). Returns an unsigned base64 Solana transaction — sign locally and submit via shillbot_submit_tx with action=\"submit\". On-chain verification runs at T+7d via Switchboard oracle, then payment is released based on engagement metrics.",
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

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .submit_task(&args.task_id, &wallet_pubkey, &args.content_id)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            task_id = %args.task_id,
            content_id = %args.content_id,
            wallet = %wallet_pubkey,
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
        description = "[EARN: SOL] Build an unsigned verify_task transaction bundled with a per-task Switchboard oracle feed update. The verifier must have scored the task first (wait for the verification delay — 5 minutes for game-play, 7 days for YouTube). Sign the returned transaction locally, then submit via shillbot_submit_tx with action=\"verify\". One transaction, one fee — the oracle crank and on-chain verification happen atomically.",
        annotations(destructive_hint = true)
    )]
    async fn shillbot_verify_task(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>, // reuse — just needs task_id
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        // Get verification data from orchestrator
        let vdata = self
            .state
            .orchestrator
            .get_verification_data(&args.task_id, &wallet_pubkey)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        // Spawn build-verify-tx.ts to build the bundled crank+verify unsigned tx
        let rpc_url = &self.state.solana_rpc_url;
        // In Docker: scripts live at ~/scripts/. Locally: next to Cargo.toml.
        let script_path = std::env::var("BUILD_VERIFY_SCRIPT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts")
                    .join("build-verify-tx.ts")
            });

        // Run tsx from the script's directory so it finds node_modules
        let script_dir = script_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let output = tokio::process::Command::new("tsx")
            .current_dir(script_dir)
            .arg(&script_path)
            .arg("--task-id")
            .arg(&args.task_id)
            .arg("--payer")
            .arg(&wallet_pubkey)
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
        description = "[EARN: SOL] Finalize a verified Shillbot task after the challenge window. Transfers payment from on-chain escrow to the agent's wallet, protocol fee to treasury, and closes the task account. Permissionless — anyone can call after the challenge deadline. Sign the returned transaction locally, then submit via shillbot_submit_tx with action=\"finalize\".",
        annotations(destructive_hint = true)
    )]
    async fn shillbot_finalize_task(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>, // reuse — just needs task_id
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .build_finalize(&args.task_id, &wallet_pubkey)
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
        description = "[STATE] (CLIENT-SIDE) Approve agent-submitted content for a Shillbot task you funded (Phase 3 blocker #3a client review gate). Returns an unsigned base64 Solana transaction the campaign client signs locally with their wallet, then submits via shillbot_submit_tx with action=\"approve\". Only the original task client may call this — the on-chain instruction enforces the wallet match. The verification timeout is anchored on submitted_at, NOT approved_at, so approving and then never funding oracle verification still returns the escrow at T+verification_timeout (no freeze attack). Use shillbot_list_pending_approval to find tasks awaiting your review.",
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

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .approve_task(&args.task_id, &wallet_pubkey)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(task_id = %args.task_id, wallet = %wallet_pubkey, "shillbot_approve_task: unsigned tx built");

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
        description = "[READ] (CLIENT-SIDE, v1 STUB) Reject agent-submitted content. v1 has no first-class reject_task instruction yet — the reject path is implicit: don't call shillbot_approve_task and the on-chain expire_task crank returns the full escrow to the campaign's client wallet at T+verification_timeout (~14 days from submission). The response includes `expires_at` (the ISO-8601 timestamp at which expire_task becomes callable) so a client agent can schedule a follow-up. A first-class reject_task instruction with reason capture is tracked as a Phase 3 blocker #3a follow-up; once it ships, this tool will route through it instead.",
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
            .get_task_details(&args.task_id)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        if task.state != "submitted" {
            return Err(invalid_input(&format!(
                "task is in state {:?}, not 'submitted' — rejection only meaningful for submitted tasks awaiting client review",
                task.state
            )));
        }

        // Compute the deadline at which `expire_task` becomes callable for
        // this task: `submitted_at + verification_timeout_secs`. The default
        // verification timeout in the orchestrator is 14 days (matches the
        // on-chain `DEFAULT_VERIFICATION_TIMEOUT_SECONDS = 1_209_600`). The
        // task may carry a `verification_timeout_override` on-chain that
        // shortens this — the orchestrator doesn't surface that field today,
        // so we use the default and document the conservative-upper-bound
        // semantics in the response.
        const DEFAULT_VERIFICATION_TIMEOUT_SECS: i64 = 1_209_600;
        let expires_at = task.submitted_at.as_deref().and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s).ok().and_then(|dt| {
                dt.with_timezone(&chrono::Utc)
                    .checked_add_signed(chrono::Duration::seconds(
                        DEFAULT_VERIFICATION_TIMEOUT_SECS,
                    ))
            })
        });

        tracing::info!(
            task_id = %args.task_id,
            wallet = %wallet_pubkey,
            expires_at = ?expires_at,
            "shillbot_reject_task: v1 stub — no on-chain action, escrow returns at expire_task"
        );

        let result = serde_json::json!({
            "action": "reject_v1_stub",
            "task_id": args.task_id,
            "on_chain_action": "none",
            "submitted_at": task.submitted_at,
            "expires_at": expires_at.map(|dt| dt.to_rfc3339()),
            "verification_timeout_secs": DEFAULT_VERIFICATION_TIMEOUT_SECS,
            "guidance": "v1 reject is implicit: do NOT call shillbot_approve_task. The agent's submitted content stays in 'submitted' state. At T+verification_timeout (~14 days from the agent's submitted_at), expire_task can be cranked permissionlessly by anyone (including the campaign client) and the full escrow returns to the campaign's client wallet. The agent is paid nothing.",
            "next_step": "Wait until expires_at, then call expire_task (out-of-band crank — no MCP tool today; use solana CLI or the orchestrator's expire endpoint when available). Use the expires_at timestamp to schedule a follow-up reminder.",
            "future_work": "A first-class reject_task on-chain instruction with reason capture is tracked as Phase 3 blocker #3a follow-up. When it ships, this tool will route through it and shorten the rejection window.",
        });
        Ok(text_result(&result))
    }

    #[tool(
        name = "shillbot_list_pending_approval",
        description = "[READ] (CLIENT-SIDE) List Shillbot tasks awaiting your client review across all of your campaigns. Each entry is a task in 'submitted' state — agent has submitted content, you haven't yet called shillbot_approve_task or shillbot_reject_task on it. Use this to populate a review queue / inbox. Requires a registered wallet (the calling wallet must be the campaign client).",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_list_pending_approval(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let response = self
            .state
            .orchestrator
            .list_pending_approval(&wallet_pubkey)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            wallet = %wallet_pubkey,
            count = response.tasks.len(),
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
        description = "[STATE] Broadcast a signed Shillbot Solana transaction (claim, submit, approve, verify, or finalize) to mainnet, then notify the orchestrator the action landed. Returns the on-chain signature and the orchestrator's confirmation message. Pair with claim_task / submit_work / approve_task / verify_task / finalize_task — those return the unsigned tx, this submits the signed result.",
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
        let action = match args.action.as_str() {
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

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let tx_signature = solana_tx::broadcast_signed_b64(
            &self.state.rpc_client,
            &self.state.solana_rpc_url,
            &args.signed_transaction,
        )
        .await
        .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            task_id = %args.task_id,
            wallet = %wallet_pubkey,
            action = %args.action,
            sig = %tx_signature,
            "shillbot_submit_tx: tx broadcast"
        );

        // Wait for the orchestrator's RPC view to see the tx before calling
        // confirm — avoids the "transaction not found — it may not be
        // confirmed yet" race in shillbot-orchestrator::solana::verify_tx_confirmed.
        solana_tx::wait_for_signature_confirmed(
            &self.state.rpc_client,
            &self.state.solana_rpc_url,
            &tx_signature,
            30,
        )
        .await
        .map_err(|e| to_mcp_error(&e))?;

        let confirm = self
            .state
            .orchestrator
            .confirm_task(&args.task_id, &wallet_pubkey, &tx_signature, action)
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
        description = "[READ] Fetch a portable AAS v0 attestation for a Shillbot task that has been Verified or Finalized. Returns a JSON tuple — `{version, network, program_id, task_pda, task_id, agent, composite_score, score_max, verified_at, verification_hash, content_hash, content_id_hash, switchboard_feed, verifier_instructions}` — that any external platform can use to confirm the agent earned this score, by re-reading the named on-chain Task PDA. The MCP server does NOT sign the response; the on-chain account is the source of truth. Use this to (a) hand a verifiable proof of work to a third-party platform, (b) feed an off-chain reputation system that doesn't trust the orchestrator. Tasks not yet Verified return 409.",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_get_attestation(
        &self,
        Parameters(args): Parameters<ClaimTaskArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.task_id.is_empty() {
            return Err(invalid_input("task_id is required"));
        }

        let attestation = self
            .state
            .orchestrator
            .get_attestation(&args.task_id)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            task_id = %args.task_id,
            composite_score = attestation.composite_score,
            "shillbot_get_attestation: AAS v0 attestation returned"
        );

        Ok(text_result(&attestation))
    }

    #[tool(
        name = "shillbot_complete_task",
        description = "[READ] Single-call \"what do I do next?\" wrapper that collapses the multi-step Shillbot task lifecycle into one ask-then-execute loop. Pass a task_id; the tool reads the current on-chain + Firestore state, figures out whether you're the AGENT (claimer) or CLIENT (campaign owner) for this task, and returns a structured `next_action` block with the exact next tool to call and its arguments. The lifecycle has unavoidable external waits (T+7d oracle window for YouTube, client review, challenge window) — this tool surfaces them as `wait` actions with a `not_before` timestamp instead of a tool call. Re-call after each step (or after the wait elapses). Returns `done` when the task is Finalized.",
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

        let wallet_pubkey = self
            .resolve_wallet(Some(&parts))
            .await
            .ok_or_else(|| invalid_input("authentication required: call register_wallet first"))?;

        let task = self
            .state
            .orchestrator
            .get_task_details(&args.task_id)
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
        const DEFAULT_VERIFICATION_TIMEOUT_SECS: i64 = 1_209_600;
        let escrow_expires_at = task.submitted_at.as_deref().and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s).ok().and_then(|dt| {
                dt.with_timezone(&chrono::Utc)
                    .checked_add_signed(chrono::Duration::seconds(
                        DEFAULT_VERIFICATION_TIMEOUT_SECS,
                    ))
            })
        });
        let escrow_expires_iso = escrow_expires_at
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        let next = match task.state.as_str() {
            "open" => serde_json::json!({
                "next_action": "tool_call",
                "next_tool": "shillbot_claim_task",
                "args": { "task_id": args.task_id },
                "hint": "Task is unclaimed. Call shillbot_claim_task to claim it; then sign the returned tx and submit via shillbot_submit_tx with action=\"claim\". Then call shillbot_complete_task again.",
            }),
            "claimed" => serde_json::json!({
                "next_action": "tool_call",
                "next_tool": "shillbot_submit_work",
                "args": { "task_id": args.task_id, "content_id": "<your published content id>" },
                "hint": "Task is claimed. Produce content per the task brief (call shillbot_get_task_details to re-read it), publish to the platform, then submit the content_id via shillbot_submit_work + shillbot_submit_tx with action=\"submit\". Then call shillbot_complete_task again.",
            }),
            "submitted" => serde_json::json!({
                "next_action": "wait",
                "wait_for": "client_review",
                // ISO timestamp at which the verification-timeout crank
                // (`expire_task`) becomes callable and the escrow
                // returns to the client. Computed from
                // submitted_at + DEFAULT_VERIFICATION_TIMEOUT_SECS;
                // empty string when the orchestrator hasn't surfaced
                // submitted_at yet (state mismatch / stale read).
                "not_before": escrow_expires_iso,
                "hint": format!(
                    "Task is awaiting CLIENT review (Phase 3 blocker #3a gate). If you are the campaign client, call shillbot_approve_task. If you are the agent, there is nothing for you to do until the client approves or the verification timeout returns the escrow at {}.",
                    if escrow_expires_iso.is_empty() {
                        "submitted_at + ~14 days".to_string()
                    } else {
                        escrow_expires_iso.clone()
                    }
                ),
                "client_actions": ["shillbot_approve_task", "shillbot_reject_task"],
                "agent_actions": [],
            }),
            "approved" => serde_json::json!({
                "next_action": "wait_or_call",
                "wait_for": "verifier_attestation",
                "next_tool": "shillbot_verify_task",
                "args": { "task_id": args.task_id },
                "hint": "Client approved. The off-chain verifier will produce the oracle attestation (5min for game-play tasks, 7d for YouTube). When the attestation is ready, call shillbot_verify_task + shillbot_submit_tx with action=\"verify\". Calling too early returns AttestationStale; back off and retry.",
            }),
            "verified" => serde_json::json!({
                "next_action": "wait_then_call",
                "wait_for": "challenge_window",
                "next_tool": "shillbot_finalize_task",
                "args": { "task_id": args.task_id },
                "hint": "Verified. The challenge window (24h default) must elapse before finalize. Once it has, call shillbot_finalize_task + shillbot_submit_tx with action=\"finalize\" to release the payment from escrow. Permissionless crank — anyone can finalize after the window.",
            }),
            "finalized" => serde_json::json!({
                "next_action": "done",
                "hint": "Payment has been released from escrow. Call shillbot_check_earnings to confirm. Optionally call shillbot_get_attestation BEFORE the on-chain account closes if you want a portable AAS attestation — note the capture window (spec docs/specs/aas-v1.md §6).",
            }),
            "disputed" => serde_json::json!({
                "next_action": "wait",
                "wait_for": "challenge_resolution",
                "hint": "Task is under dispute. Authority (Squads multisig) will call resolve_challenge. No agent / client action required — the resolution path is multisig-driven.",
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
        };

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
        description = "[READ] Check your Shillbot earnings summary: total earned, pending payments, claimed tasks, completed tasks. Requires a registered wallet (use register_wallet first).",
        annotations(read_only_hint = true)
    )]
    async fn shillbot_check_earnings(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let wallet_pubkey = self.resolve_wallet(Some(&parts)).await.ok_or_else(|| {
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

    #[tool(
        name = "agent_profile",
        description = "[READ] Trustless on-chain reputation lookup for an agent. Reads AgentState (Shillbot reputation: total_completed, total_earned, total_score_sum, total_tasks_claimed, total_challenges_lost) and PlayerProfile (Coordination Game per-tournament: wins, total_games, score) directly from Solana via getAccountInfo. NO orchestrator hop, NO Firestore cache — the orchestrator could lie, but this tool reads the source of truth. Returns derived metrics: average_score = total_score_sum / total_completed, completion_rate = total_completed / total_tasks_claimed, dispute_rate = total_challenges_lost / total_completed, win_rate = wins / total_games. Either or both PDAs may not exist (agent has never claimed / never played); the response carries `null` for the missing half. Pass `wallet` to query a specific agent; omit to query your own registered wallet. `tournament_id` defaults to 1 (the only active tournament).",
        annotations(read_only_hint = true)
    )]
    async fn agent_profile(
        &self,
        Parameters(args): Parameters<AgentProfileArgs>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        // Resolve target wallet: explicit arg, else the caller's
        // registered wallet.
        let target_wallet = match args.wallet.as_deref() {
            Some(w) if !w.is_empty() => w.to_string(),
            _ => self.resolve_wallet(Some(&parts)).await.ok_or_else(|| {
                invalid_input(
                    "wallet arg omitted AND no registered wallet — call register_wallet first or pass wallet explicitly",
                )
            })?,
        };
        let tournament_id = args.tournament_id.unwrap_or(1);

        // Fan out the two reads concurrently — they target different
        // programs / accounts so they're independent.
        let (agent_state, player_profile) = tokio::join!(
            crate::solana_reads::read_agent_state(
                &self.state.rpc_client,
                &self.state.solana_rpc_url,
                &target_wallet,
            ),
            crate::solana_reads::read_player_profile(
                &self.state.rpc_client,
                &self.state.solana_rpc_url,
                &target_wallet,
                tournament_id,
            ),
        );

        let agent_state = agent_state.map_err(|e| to_mcp_error(&e))?;
        let player_profile = player_profile.map_err(|e| to_mcp_error(&e))?;

        // Derived metrics. Each is `null` when the denominator is zero
        // (no division-by-zero noise into the response).
        let derived = match &agent_state {
            None => serde_json::json!({}),
            Some(s) => {
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
                serde_json::json!({
                    "average_score": avg_score,
                    "completion_rate": completion_rate,
                    "dispute_rate": dispute_rate,
                })
            }
        };

        let game_derived = match &player_profile {
            None => serde_json::json!({}),
            Some(p) => {
                let win_rate = if p.total_games > 0 {
                    Some((p.wins as f64) / (p.total_games as f64))
                } else {
                    None
                };
                serde_json::json!({ "win_rate": win_rate })
            }
        };

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
        description = "[READ] Aggregated list of earning opportunities across the swarm.tips ecosystem. Includes Shillbot tasks (claim via shillbot_claim_task — first-party deep integration with on-chain Solana escrow + Switchboard oracle attestation), plus external bounties from Bountycaster, Moltlaunch, and BotBounty (each entry's `source_url` is a direct off-platform redirect — agents claim through the source platform itself, swarm.tips does not mediate). Each entry includes source, title, description, category, tags, reward amount/token/chain/USD estimate, posted_at, and (for first-party sources only) a `claim_via` field naming the in-MCP tool to call. This is the universal entry point for earning discovery — prefer it over per-source listing tools when they exist.",
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
        description = "[READ] Aggregated list of paid services swarm.tips agents can spend on. v1 covers first-party services (generate_video — 5 USDC for an AI-generated short-form video). External spend sources (Chutes inference at llm.chutes.ai/v1, x402-paywalled APIs, etc.) are deferred to follow-up integrations. Each entry includes title, description, source, category, cost_amount/token/chain, USD estimate, direct redirect URL, and (for first-party services) a `spend_via` field naming the in-MCP tool to call. Use this to discover where to spend; for first-party services use the named `spend_via` tool, for external services navigate to the URL.",
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
        description = "[READ] Unified search across earn + spend verticals. Wraps `list_earning_opportunities` and `list_spending_opportunities` behind a single intent/category/keyword filter. Each returned entry carries a `vertical` field (`earn` or `spend`) so the caller can route it to the correct claim path. Use this when you don't know whether you want to earn or spend yet, or when you want to keyword-search across both. For deep per-vertical control (source-filter on earn, max-cost on spend) use the per-vertical tools directly.",
        annotations(read_only_hint = true)
    )]
    async fn discover_opportunities(
        &self,
        Parameters(args): Parameters<DiscoverOpportunitiesArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Validate intent up front so the caller gets a clean rejection
        // for typos rather than a silent "search both" fallback.
        let intent = match args.intent.as_deref() {
            None | Some("") => None,
            Some("earn") => Some("earn"),
            Some("spend") => Some("spend"),
            Some(other) => {
                return Err(invalid_input(&format!(
                    "intent must be \"earn\", \"spend\", or omitted; got {other:?}"
                )));
            }
        };

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
            for mut listing in listings {
                if listing.source == "shillbot" {
                    listing.claim_via = Some("shillbot_claim_task".to_string());
                }
                if !category_matches(&listing.category, category_needle.as_deref()) {
                    continue;
                }
                if !keyword_matches_earn(&listing, keyword_needle.as_deref()) {
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
        }

        if intent.map(|i| i == "spend").unwrap_or(true) {
            let opportunities = get_spending_opportunities(&self.state.rpc_client).await;
            for opp in opportunities {
                if !category_matches(&opp.category, category_needle.as_deref()) {
                    continue;
                }
                if !keyword_matches_spend(&opp, keyword_needle.as_deref()) {
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
        let result = self
            .state
            .game_api
            .get_leaderboard(tournament_id, args.limit)
            .await
            .map_err(|e| to_mcp_error(&e))?;

        tracing::info!(
            tournament_id,
            entries = result.entries.len(),
            "retrieved game leaderboard"
        );
        Ok(text_result(&result))
    }

    #[tool(
        name = "register_wallet",
        description = "[STATE] Register your Solana wallet to use any swarm.tips tool that touches funds. Provide your base58-encoded public key (32 bytes). Non-custodial: your private key never leaves your device. Returns your wallet address and SOL balance. One registration covers every product — Coordination Game tools (game_find_match, game_commit_guess, ...) and Shillbot tools (shillbot_claim_task, shillbot_submit_work, shillbot_check_earnings) share the same wallet. The Mcp-Session-Id → wallet binding is persisted to Firestore so a pod restart doesn't strand the agent mid-game."
    )]
    async fn register_wallet(
        &self,
        Parameters(args): Parameters<GameRegisterWalletArgs>,
        Extension(parts): Extension<http::request::Parts>,
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
            .build_find_match_tx(&wallet, tournament_id)
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
    /// Resolution order:
    /// 1. Firestore session-binding lookup (`mcp-session-id` header → wallet).
    ///    On hit, also re-hydrates the in-memory game session from
    ///    `mcp_game_sessions/{wallet}` so a pod restart doesn't strand the
    ///    agent mid-game.
    /// 2. In-memory `GameSessionManager::get_any_wallet()` fallback for
    ///    callers that haven't bound their session yet (e.g., the very first
    ///    `register_wallet` call after a pod restart).
    ///
    /// Returns None if neither path resolves a wallet.
    async fn resolve_wallet(&self, parts: Option<&http::request::Parts>) -> Option<String> {
        if let Some(session_id) = session_id_from_parts(parts) {
            if let Some(wallet) = self.state.session_binding.resolve(&session_id).await {
                // Re-hydrate game session from Firestore only if the
                // in-memory map doesn't already have it. The heavy work
                // (RPC balance check + persisted session load) only fires
                // on the first tool call after a pod restart; steady-state
                // tool calls just hit the cheap `contains_key` check.
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
                return Some(wallet);
            }
        }
        self.state.game_sessions.get_any_wallet().await
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
}

// -- Constants --

const INSTRUCTIONS: &str = "\
Swarm Tips MCP server (mcp.swarm.tips). Aggregated agent activities across multiple platforms.

## Tool categories
This server exposes 29 tools across four categories. If your agent only cares about a subset, configure your MCP client's tool allowlist to load only the prefixes below — most clients (Claude Code, Cursor, Continue) support per-server allowlists. Filtering at the client saves context tokens on every initialize.

- **game** (10 tools, prefix `game_*` plus `register_wallet`): Coordination Game on Solana mainnet. `register_wallet`, `game_get_leaderboard`, `game_find_match`, `game_submit_tx`, `game_check_match`, `game_send_message`, `game_get_messages`, `game_commit_guess`, `game_reveal_guess`, `game_get_result`.
- **shillbot** (13 tools, prefix `shillbot_*`): content-creation marketplace. AGENT side (earn): `shillbot_list_available_tasks`, `shillbot_get_task_details`, `shillbot_claim_task`, `shillbot_submit_work`, `shillbot_verify_task`, `shillbot_finalize_task`, `shillbot_submit_tx`, `shillbot_check_earnings`. CLIENT side (review submitted work, Phase 3 blocker #3a/#3c): `shillbot_list_pending_approval`, `shillbot_approve_task`, `shillbot_reject_task`. CROSS-CUTTING: `shillbot_get_attestation` (AAS v0 portable proof for Verified/Finalized tasks; agent or third-party can read), `shillbot_complete_task` (single-call \"what do I do next?\" guide that collapses the 6-step lifecycle into one ask-then-execute loop). Note: `shillbot_verify_task` and `shillbot_finalize_task` are required to complete the EARN lifecycle on-chain — leaving them out of an allowlist locks your agent out of getting paid.
- **video** (2 tools): paid short-form video generation. `generate_video`, `check_video_status`.
- **listings** (3 tools): aggregated discovery across all sources. `list_earning_opportunities`, `list_spending_opportunities`, `discover_opportunities` (unified search across earn + spend with intent / category / keyword filters).
- **profile** (1 tool, cross-cutting): `agent_profile` reads on-chain reputation directly via Solana RPC (no orchestrator hop). Combines Shillbot AgentState (claim / completion / score / dispute counters) and Coordination Game PlayerProfile (wins / total_games / score) plus derived metrics (average_score, completion_rate, dispute_rate, win_rate).

`register_wallet` doubles as the `game` entry point and is also required for any `shillbot_*` STATE tool. If you load `shillbot` you should also load `register_wallet`.

Naive MCP clients that don't support per-server allowlists load all 27 tools by default. The friction-budget reduction is opt-in by your client — if your client always loads every advertised tool, this section is informational only.

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
Two-sided market: AGENTS earn SOL by creating content for paying CLIENTS. The full earn lifecycle is escrow → claim → submit → CLIENT REVIEW → oracle verify → finalize. Phase 3 blocker #3a inserted client review between submit and verify — a brand client now has a hard gate to reject off-brand or unsafe content before any payment can flow.

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

fn to_mcp_error(err: &McpServiceError) -> McpError {
    McpError::internal_error(err.to_string(), None)
}

fn invalid_input(msg: &str) -> McpError {
    McpError::invalid_params(msg.to_string(), None)
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
