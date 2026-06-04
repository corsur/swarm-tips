use crate::errors::McpServiceError;
use serde::{Deserialize, Serialize};

const MAX_TASK_LIMIT: u32 = 100;

/// Build the `?network=<n>` query suffix the orchestrator uses to dispatch
/// per-network state. Returns an empty string when `network` is `None`,
/// preserving the orchestrator's mainnet-default behaviour. The suffix
/// always starts with `?`; callers that need to append to a URL that
/// already carries a query string should build the `&network=...` token
/// inline (currently only `list_tasks`, which already has `?limit=...`).
fn network_query_suffix(network: Option<&str>) -> String {
    // Precondition: when present, the network token must not be empty —
    // otherwise we'd emit `?network=` and the orchestrator would route to
    // its default rather than rejecting the request.
    debug_assert!(
        network.map(|n| !n.is_empty()).unwrap_or(true),
        "network must be None or a non-empty token"
    );
    let result = match network {
        Some(n) => format!("?network={}", urlencoding::encode(n)),
        None => String::new(),
    };
    // Postcondition: an emitted suffix is well-formed (`?network=...`),
    // never just `?network=` with an empty value.
    debug_assert!(
        result.is_empty() || result.starts_with("?network="),
        "suffix must be empty or start with `?network=`"
    );
    result
}

/// HTTP client for proxying read operations to the orchestrator API.
pub struct OrchestratorProxy {
    client: reqwest::Client,
    base_url: String,
    /// Base URL for the shorts/video service (shorts-api), which was split out
    /// of shillbot-api. The mcp-server is in-cluster and calls services
    /// directly, so it bypasses the edge path-routing and must target shorts-api
    /// explicitly for the `/shorts/*` endpoints (generate_video,
    /// check_video_status). Task/campaign endpoints still use `base_url`.
    shorts_base_url: String,
}

/// One task as returned by the orchestrator's `GET /tasks` and
/// `GET /tasks/:id` endpoints. Field names mirror the orchestrator's wire
/// format (`shillbot-api::models::task::TaskResponse`) so serde can
/// deserialize directly. Optional fields are defaulted so a missing key from
/// the upstream doesn't fail the whole response.
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task_id: String,
    #[serde(default)]
    pub campaign_id: Option<String>,
    #[serde(default)]
    pub campaign_topic: Option<String>,
    pub state: String,
    #[serde(default)]
    pub platform: Option<u8>,
    #[serde(default)]
    pub estimated_payment_lamports: Option<u64>,
    #[serde(default)]
    pub quality_threshold: Option<u64>,
    #[serde(default)]
    pub created_at: Option<String>,
    /// ISO-8601 timestamp the agent called `submit_work` on-chain. Required
    /// by `shillbot_reject_task` to compute the verification-timeout deadline
    /// (`expires_at = submitted_at + verification_timeout_secs`).
    #[serde(default)]
    pub submitted_at: Option<String>,
    /// Agent wallet that claimed this task. `null` until claim_task confirms.
    /// Surfaced 2026-05-11 by the mcp-shillbot-lifecycle e2e — without this
    /// field, agents calling shillbot_get_task_details couldn't see who
    /// claimed the task they just claimed.
    #[serde(default)]
    pub agent: Option<String>,
    /// ISO-8601 timestamp of claim_task on-chain confirmation.
    #[serde(default)]
    pub claimed_at: Option<String>,
    /// ISO-8601 timestamp of approve_task on-chain confirmation (for
    /// `requires_approval` campaigns).
    #[serde(default)]
    pub approved_at: Option<String>,
    /// Agent-submitted proof: YouTube video ID, tweet ID, game session
    /// ID, etc. Populated once submit_work confirms.
    #[serde(default)]
    pub content_id: Option<String>,
    /// Switchboard-attested score (0..=1_000_000 scaled). Populated after
    /// the verifier records the oracle attestation.
    #[serde(default)]
    pub composite_score: Option<u64>,
    /// Lamports paid out to the agent. Populated after finalize_task
    /// confirms — `payment_amount > 0` is the canonical "did this task
    /// actually pay" signal for downstream callers.
    #[serde(default)]
    pub payment_amount: Option<u64>,
    /// Per-task scoring weights (platform-specific shape). Kept raw so the
    /// MCP layer doesn't have to track scoring-config schema drift.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scoring_scales: Option<serde_json::Value>,
    /// Full campaign brief if the orchestrator joined it in. Kept as a raw
    /// JSON value because the brief shape varies by platform and we just want
    /// to forward it to the agent verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Single-task details. The orchestrator returns the same `TaskResponse`
/// shape from `GET /tasks/:id` as it does from list — so we reuse the
/// `TaskSummary` deserializer rather than maintaining a parallel struct.
pub type TaskDetails = TaskSummary;

/// Mirrors `shillbot-api::models::subsidy::EarningsResponse`
/// at `coordination-app/backend/shillbot-api/src/models/subsidy.rs`.
/// Field-for-field match is required — any drift breaks
/// `shillbot_check_earnings` over MCP. The old shape (`total_earned`,
/// `pending_tasks`) was stale; the orchestrator never returned those
/// fields, just nobody was hitting this path over MCP until 2026-05-11.
#[derive(Debug, Serialize, Deserialize)]
pub struct EarningsResponse {
    pub total_earned_lamports: u64,
    pub total_subsidy_lamports: u64,
    pub tasks_completed: u64,
    pub average_score: f64,
}

/// Mirrors `shillbot-api::models::task::TransactionResponse`. Returned
/// by `POST /tasks/:id/claim` and `POST /tasks/:id/submit` — the orchestrator
/// builds the unsigned Solana transaction and the agent signs locally.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub message: String,
    pub task_id: String,
    /// Base64-encoded unsigned Solana transaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_pda: Option<String>,
}

/// AAS v0 attestation — mirrors orchestrator
/// `shillbot-api::models::task::AttestationResponse`. A portable,
/// structured proof an agent earned a specific composite score on a
/// specific Task PDA. Verifiable off-chain by re-reading the named PDA.
#[derive(Debug, Serialize, Deserialize)]
pub struct AttestationResponse {
    pub version: String,
    pub network: String,
    pub program_id: String,
    pub task_pda: String,
    pub task_id: u64,
    pub client: String,
    pub agent: String,
    pub state: String,
    pub platform: u8,
    pub composite_score: u64,
    pub score_max: u64,
    pub verified_at: String,
    pub verification_hash: String,
    pub content_hash: String,
    pub content_id_hash: String,
    pub switchboard_feed: String,
    pub verifier_instructions: String,
}

/// Verification data needed to build a bundled crank+verify transaction.
/// Mirrors `shillbot-api::models::task::VerificationDataResponse`.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationDataResponse {
    pub task_id: String,
    pub task_pda: String,
    pub composite_score: u64,
    pub verification_hash: String,
    pub global_state: String,
    pub switchboard_feed: String,
}

/// Action discriminator for `POST /tasks/:id/confirm`. Mirrors
/// `shillbot-api::models::task::ConfirmAction` — must serialize as
/// snake_case to match the orchestrator's `#[serde(rename_all = "snake_case")]`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmAction {
    Claim,
    Submit,
    /// Client signed `approve_task` on-chain.
    Approve,
    Verify,
    Finalize,
}

/// Mirrors `shillbot-api::models::task::ConfirmTaskResponse`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfirmTaskResponse {
    pub task_id: String,
    pub action: String,
    pub message: String,
}

impl OrchestratorProxy {
    pub fn new(base_url: String, shorts_base_url: String) -> Self {
        assert!(
            !base_url.is_empty(),
            "orchestrator base_url must not be empty"
        );
        assert!(
            !shorts_base_url.is_empty(),
            "shorts base_url must not be empty"
        );

        // 60s timeout: most orchestrator endpoints respond in <500ms, but
        // /shorts/create-crypto runs a Grok prompt expansion + Workflows
        // kickoff inside the request handler that legitimately takes
        // 10-15s. The previous 10s cap caused reqwest to drop the
        // connection mid-handler, axum cancelled the in-flight handler at
        // the next await, and a paid x402 session was stranded in
        // Firestore with status=Generating but no actual workflow ever
        // triggered. 60s is generous enough for any orchestrator endpoint
        // we currently call without adding meaningful latency to the fast
        // paths (tasks list / claim / earnings — all <500ms).
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();

        Self {
            client,
            base_url,
            shorts_base_url,
        }
    }

    /// List available tasks from the orchestrator.
    pub async fn list_tasks(
        &self,
        limit: Option<u32>,
        min_price: Option<u64>,
        network: Option<&str>,
    ) -> Result<TaskListResponse, McpServiceError> {
        let effective_limit = limit.unwrap_or(20).min(MAX_TASK_LIMIT);

        let mut url = format!("{}/tasks?limit={effective_limit}", self.base_url);
        if let Some(price) = min_price {
            url.push_str(&format!("&min_price={price}"));
        }
        if let Some(n) = network {
            url.push_str(&format!("&network={}", urlencoding::encode(n)));
        }

        let response = self.client.get(&url).send().await.map_err(|e| {
            tracing::error!(service = "coordination-mcp-server", error = %e, url = %url, "orchestrator list_tasks request failed");
            McpServiceError::OrchestratorError(format!("request failed: {e}"))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "coordination-mcp-server", status = %status, body = %body, "orchestrator returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        let result: TaskListResponse = response.json().await.map_err(|e| {
            tracing::error!(service = "coordination-mcp-server", error = %e, "failed to parse orchestrator task list response");
            McpServiceError::OrchestratorError(format!("invalid response: {e}"))
        })?;

        if result.tasks.len() > MAX_TASK_LIMIT as usize {
            tracing::warn!(
                count = result.tasks.len(),
                limit = MAX_TASK_LIMIT,
                "orchestrator returned more tasks than requested limit"
            );
        }

        Ok(result)
    }

    /// Get full details for a specific task.
    pub async fn get_task_details(
        &self,
        task_id: &str,
        network: Option<&str>,
    ) -> Result<TaskDetails, McpServiceError> {
        if task_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_id must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/tasks/{task_id}{qs}", self.base_url);

        let response = self.client.get(&url).send().await.map_err(|e| {
            tracing::error!(service = "coordination-mcp-server", error = %e, task_id = %task_id, "orchestrator get_task_details failed");
            McpServiceError::OrchestratorError(format!("request failed: {e}"))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "coordination-mcp-server", status = %status, task_id = %task_id, "orchestrator returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        let details: TaskDetails = response.json().await.map_err(|e| {
            tracing::error!(service = "coordination-mcp-server", error = %e, task_id = %task_id, "failed to parse task details");
            McpServiceError::OrchestratorError(format!("invalid response: {e}"))
        })?;

        if details.task_id.is_empty() {
            return Err(McpServiceError::OrchestratorError(
                "task_id must not be empty in response".to_string(),
            ));
        }

        Ok(details)
    }

    /// Get earnings summary for the authenticated agent.
    pub async fn get_earnings(
        &self,
        wallet_pubkey: &str,
        network: Option<&str>,
    ) -> Result<EarningsResponse, McpServiceError> {
        if wallet_pubkey.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "wallet_pubkey must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/agent/earnings{qs}", self.base_url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(wallet_pubkey)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(service = "coordination-mcp-server", error = %e, wallet = %wallet_pubkey, "orchestrator get_earnings failed");
                McpServiceError::OrchestratorError(format!("request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "coordination-mcp-server", status = %status, wallet = %wallet_pubkey, "orchestrator returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        let earnings: EarningsResponse = response.json().await.map_err(|e| {
            tracing::error!(service = "coordination-mcp-server", error = %e, wallet = %wallet_pubkey, "failed to parse earnings response");
            McpServiceError::OrchestratorError(format!("invalid response: {e}"))
        })?;

        if earnings.tasks_completed == 0 && earnings.average_score != 0.0 {
            tracing::warn!(
                wallet = %wallet_pubkey,
                average_score = earnings.average_score,
                "orchestrator returned non-zero average_score with zero tasks completed"
            );
        }

        Ok(earnings)
    }

    /// Request the orchestrator build an unsigned `claim_task` Solana transaction
    /// for `wallet_pubkey`. The orchestrator looks up the task PDA, derives the
    /// client wallet from on-chain state, and returns a base64-encoded unsigned
    /// transaction the agent must sign locally.
    pub async fn claim_task(
        &self,
        task_id: &str,
        wallet_pubkey: &str,
        network: Option<&str>,
    ) -> Result<TransactionResponse, McpServiceError> {
        if task_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_id must not be empty".to_string(),
            ));
        }
        if wallet_pubkey.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "wallet_pubkey must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/tasks/{task_id}/claim{qs}", self.base_url);
        let response = self
            .client
            .post(&url)
            .bearer_auth(wallet_pubkey)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(service = "mcp-server", error = %e, task_id = %task_id, "orchestrator claim_task failed");
                McpServiceError::OrchestratorError(format!("request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "mcp-server", status = %status, task_id = %task_id, "orchestrator claim_task returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        let parsed: TransactionResponse = response.json().await.map_err(|e| {
            McpServiceError::OrchestratorError(format!("invalid claim_task response: {e}"))
        })?;

        if parsed.transaction.is_none() {
            return Err(McpServiceError::OrchestratorError(
                "claim_task response missing transaction field".to_string(),
            ));
        }

        Ok(parsed)
    }

    /// Request the orchestrator build an unsigned `submit_work` Solana
    /// transaction. The orchestrator persists the content_id on the task doc
    /// before returning so it survives the confirm round-trip.
    pub async fn submit_task(
        &self,
        task_id: &str,
        wallet_pubkey: &str,
        content_id: &str,
        network: Option<&str>,
    ) -> Result<TransactionResponse, McpServiceError> {
        if task_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_id must not be empty".to_string(),
            ));
        }
        if wallet_pubkey.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "wallet_pubkey must not be empty".to_string(),
            ));
        }
        if content_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "content_id must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/tasks/{task_id}/submit{qs}", self.base_url);
        let body = serde_json::json!({ "content_id": content_id });
        let response = self
            .client
            .post(&url)
            .bearer_auth(wallet_pubkey)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(service = "mcp-server", error = %e, task_id = %task_id, "orchestrator submit_task failed");
                McpServiceError::OrchestratorError(format!("request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "mcp-server", status = %status, task_id = %task_id, "orchestrator submit_task returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        let parsed: TransactionResponse = response.json().await.map_err(|e| {
            McpServiceError::OrchestratorError(format!("invalid submit_task response: {e}"))
        })?;

        if parsed.transaction.is_none() {
            return Err(McpServiceError::OrchestratorError(
                "submit_task response missing transaction field".to_string(),
            ));
        }

        Ok(parsed)
    }

    /// Request the orchestrator build an unsigned `verify_task` Solana
    /// transaction. Bundles the Switchboard feed read + composite score
    /// into the on-chain verify instruction. The agent signs and submits.
    pub async fn get_verification_data(
        &self,
        task_id: &str,
        wallet_pubkey: &str,
        network: Option<&str>,
    ) -> Result<VerificationDataResponse, McpServiceError> {
        if task_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_id must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/tasks/{task_id}/build-verify{qs}", self.base_url);
        let response = self
            .client
            .post(&url)
            .bearer_auth(wallet_pubkey)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(service = "mcp-server", error = %e, task_id = %task_id, "orchestrator verification-data failed");
                McpServiceError::OrchestratorError(format!("request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        response.json().await.map_err(|e| {
            McpServiceError::OrchestratorError(format!("invalid verification-data response: {e}"))
        })
    }

    /// Request the orchestrator build an unsigned `finalize_task` Solana
    /// transaction. Permissionless after the challenge deadline.
    pub async fn build_finalize(
        &self,
        task_id: &str,
        wallet_pubkey: &str,
        network: Option<&str>,
    ) -> Result<TransactionResponse, McpServiceError> {
        if task_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_id must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/tasks/{task_id}/build-finalize{qs}", self.base_url);
        let response = self
            .client
            .post(&url)
            .bearer_auth(wallet_pubkey)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(service = "mcp-server", error = %e, task_id = %task_id, "orchestrator build-finalize failed");
                McpServiceError::OrchestratorError(format!("request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        response.json().await.map_err(|e| {
            McpServiceError::OrchestratorError(format!("invalid build-finalize response: {e}"))
        })
    }

    /// Request the orchestrator build an unsigned `approve_task` Solana
    /// transaction for the campaign client. The orchestrator verifies the
    /// calling wallet IS the campaign's client (mirrors the on-chain
    /// `NotTaskClient` check) and returns a base64-encoded unsigned
    /// transaction the client signs locally.
    pub async fn approve_task(
        &self,
        task_id: &str,
        wallet_pubkey: &str,
        network: Option<&str>,
    ) -> Result<TransactionResponse, McpServiceError> {
        if task_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_id must not be empty".to_string(),
            ));
        }
        if wallet_pubkey.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "wallet_pubkey must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/tasks/{task_id}/approve{qs}", self.base_url);
        let response = self
            .client
            .post(&url)
            .bearer_auth(wallet_pubkey)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(service = "mcp-server", error = %e, task_id = %task_id, "orchestrator approve_task failed");
                McpServiceError::OrchestratorError(format!("request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "mcp-server", status = %status, task_id = %task_id, "orchestrator approve_task returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        let parsed: TransactionResponse = response.json().await.map_err(|e| {
            McpServiceError::OrchestratorError(format!("invalid approve_task response: {e}"))
        })?;

        if parsed.transaction.is_none() {
            return Err(McpServiceError::OrchestratorError(
                "approve_task response missing transaction field".to_string(),
            ));
        }

        Ok(parsed)
    }

    /// AAS v0: fetch the portable attestation tuple for a Verified or
    /// Finalized task. Public read — no wallet binding needed because
    /// the underlying on-chain data is already public. Returns 409 from
    /// the orchestrator if the task isn't yet attested.
    pub async fn get_attestation(
        &self,
        task_id: &str,
        network: Option<&str>,
    ) -> Result<AttestationResponse, McpServiceError> {
        if task_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_id must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/tasks/{task_id}/attestation{qs}", self.base_url);
        let response = self.client.get(&url).send().await.map_err(|e| {
            tracing::error!(service = "mcp-server", error = %e, task_id = %task_id, "orchestrator get_attestation failed");
            McpServiceError::OrchestratorError(format!("request failed: {e}"))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "mcp-server", status = %status, task_id = %task_id, "orchestrator get_attestation returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        response.json().await.map_err(|e| {
            McpServiceError::OrchestratorError(format!("invalid attestation response: {e}"))
        })
    }

    /// AAS v0: fetch the portable attestation tuple by on-chain Task PDA.
    /// The PDA is the canonical AAS identifier — derivable from any public
    /// indexer of the `TaskCreated` event, so third-party verifiers don't
    /// need orchestrator access (unlike the legacy task_id route, which
    /// resolved against Firestore document IDs they couldn't know).
    pub async fn get_attestation_by_pda(
        &self,
        task_pda: &str,
        network: Option<&str>,
    ) -> Result<AttestationResponse, McpServiceError> {
        if task_pda.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_pda must not be empty".to_string(),
            ));
        }
        // Light client-side validation: PDAs are base58-encoded 32-byte
        // pubkeys, so 32-44 chars after encoding. Tighter parsing happens
        // server-side; this just rejects obvious junk early.
        if !(32..=44).contains(&task_pda.len()) {
            return Err(McpServiceError::InvalidInput(format!(
                "task_pda length {} outside expected base58 pubkey range 32..=44",
                task_pda.len()
            )));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/tasks/by-pda/{task_pda}/attestation{qs}", self.base_url);
        let response = self.client.get(&url).send().await.map_err(|e| {
            tracing::error!(service = "mcp-server", error = %e, task_pda = %task_pda, "orchestrator get_attestation_by_pda failed");
            McpServiceError::OrchestratorError(format!("request failed: {e}"))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "mcp-server", status = %status, task_pda = %task_pda, "orchestrator get_attestation_by_pda returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        response.json().await.map_err(|e| {
            McpServiceError::OrchestratorError(format!("invalid attestation response: {e}"))
        })
    }

    /// List pending-approval tasks across all of the calling client's
    /// campaigns. Mirrors orchestrator `GET /client/pending-approval`.
    pub async fn list_pending_approval(
        &self,
        wallet_pubkey: &str,
        network: Option<&str>,
    ) -> Result<TaskListResponse, McpServiceError> {
        if wallet_pubkey.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "wallet_pubkey must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/client/pending-approval{qs}", self.base_url);
        let response = self
            .client
            .get(&url)
            .bearer_auth(wallet_pubkey)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(service = "mcp-server", error = %e, "orchestrator list_pending_approval failed");
                McpServiceError::OrchestratorError(format!("request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "mcp-server", status = %status, "orchestrator list_pending_approval returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        response.json().await.map_err(|e| {
            McpServiceError::OrchestratorError(format!(
                "invalid list_pending_approval response: {e}"
            ))
        })
    }

    /// Notify the orchestrator that a `claim_task` or `submit_work` transaction
    /// landed on-chain. The orchestrator verifies the signature, deduplicates,
    /// and advances the task state in Firestore.
    pub async fn confirm_task(
        &self,
        task_id: &str,
        wallet_pubkey: &str,
        tx_signature: &str,
        action: ConfirmAction,
        network: Option<&str>,
    ) -> Result<ConfirmTaskResponse, McpServiceError> {
        if task_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_id must not be empty".to_string(),
            ));
        }
        if wallet_pubkey.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "wallet_pubkey must not be empty".to_string(),
            ));
        }
        if tx_signature.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "tx_signature must not be empty".to_string(),
            ));
        }

        let qs = network_query_suffix(network);
        let url = format!("{}/tasks/{task_id}/confirm{qs}", self.base_url);
        let body = serde_json::json!({
            "tx_signature": tx_signature,
            "action": action,
        });
        let response = self
            .client
            .post(&url)
            .bearer_auth(wallet_pubkey)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(service = "mcp-server", error = %e, task_id = %task_id, "orchestrator confirm_task failed");
                McpServiceError::OrchestratorError(format!("request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(service = "mcp-server", status = %status, task_id = %task_id, "orchestrator confirm_task returned error");
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        response.json().await.map_err(|e| {
            McpServiceError::OrchestratorError(format!("invalid confirm_task response: {e}"))
        })
    }

    /// Create a short-form video via the crypto endpoint.
    ///
    /// The orchestrator's x402 v2 middleware uses a different wire format
    /// from v1: a 402 response carries the payment requirements in the
    /// `Payment-Required` response header (base64-encoded JSON), with an
    /// EMPTY body. The agent supplies the signed payment payload via the
    /// `Payment-Signature` request header (also base64-encoded JSON of an
    /// `x402_types::v2::PaymentPayload`). Both header names are different
    /// from v1 (`X-PAYMENT` request, body-only response) — the previous v1
    /// integration was rejected by CDP for unrelated SVM-client interop
    /// reasons, and switching to v2 sidesteps the entire mess.
    pub async fn create_short_crypto(
        &self,
        prompt: &str,
        url: Option<&str>,
        tx_signature: Option<&str>,
    ) -> Result<serde_json::Value, McpServiceError> {
        let endpoint = format!("{}/shorts/create-crypto", self.shorts_base_url);
        let body = build_create_short_body(prompt, url);

        let mut req = self.client.post(&endpoint).json(&body);

        if let Some(sig) = tx_signature {
            tracing::debug!(
                service = "mcp-server",
                payment_header_len = sig.len(),
                "x402 v2: attaching Payment-Signature header"
            );
            req = req.header("Payment-Signature", sig);
        }

        let response = req
            .send()
            .await
            .map_err(|e| log_create_short_request_failure(&e, tx_signature))?;

        let status = response.status();

        // x402 v2: a 402 response carries the payment requirements in the
        // `Payment-Required` header (base64-encoded JSON), with an empty body.
        // We surface them to the agent in the same `payment_details` shape the
        // tool's wrapper uses, so the MCP `generate_video` contract is stable.
        if status.as_u16() == 402 {
            return parse_payment_required_response(response).await;
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpServiceError::OrchestratorError(format!("invalid response: {e}")))?;

        if !status.is_success() {
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        Ok(body)
    }

    /// Check the status of a short-form video generation.
    pub async fn get_short_status(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, McpServiceError> {
        if session_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "session_id must not be empty".to_string(),
            ));
        }

        let url = format!("{}/shorts/{session_id}", self.shorts_base_url);

        let response = self.client.get(&url).send().await.map_err(|e| {
            tracing::error!(service = "mcp-server", error = %e, session_id = %session_id, "orchestrator get_short_status failed");
            McpServiceError::OrchestratorError(format!("request failed: {e}"))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(McpServiceError::OrchestratorError(format!(
                "status {status}: {body}"
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpServiceError::OrchestratorError(format!("invalid response: {e}")))?;

        Ok(result)
    }
}

/// Build the JSON body for `POST /shorts/create-crypto`. Always includes the
/// prompt; conditionally includes the URL when present.
fn build_create_short_body(prompt: &str, url: Option<&str>) -> serde_json::Value {
    debug_assert!(
        !prompt.is_empty(),
        "prompt must be non-empty by handler validation"
    );
    let mut body = serde_json::json!({ "prompt": prompt });
    if let Some(u) = url {
        body["url"] = serde_json::Value::String(u.to_string());
    }
    debug_assert!(body.get("prompt").is_some(), "body must contain prompt");
    body
}

/// Walk the `std::error::Error::source` chain on a reqwest builder/transport
/// error so logs surface the underlying cause (`reqwest::Error::Builder`'s
/// `Display` impl collapses to literally "builder error" with no source). The
/// returned `McpServiceError` carries both the top-level message and the chain.
fn log_create_short_request_failure(
    e: &reqwest::Error,
    tx_signature: Option<&str>,
) -> McpServiceError {
    let mut chain = String::new();
    let mut src: Option<&dyn std::error::Error> = std::error::Error::source(e);
    while let Some(s) = src {
        if !chain.is_empty() {
            chain.push_str(" -> ");
        }
        chain.push_str(&s.to_string());
        src = s.source();
    }
    tracing::error!(
        service = "mcp-server",
        error = %e,
        error_debug = ?e,
        error_chain = %chain,
        payment_header_present = tx_signature.is_some(),
        payment_header_len = tx_signature.map(str::len).unwrap_or(0),
        "orchestrator create_short_crypto failed"
    );
    McpServiceError::OrchestratorError(format!("request failed: {e} ({chain})"))
}

/// Parse an x402 v2 `402 Payment Required` response. The v2 protocol carries
/// the requirements in the `Payment-Required` response header (base64-encoded
/// JSON) with an empty body. Returns the canonical `payment_required` JSON
/// envelope so the MCP `generate_video` tool surface stays stable.
async fn parse_payment_required_response(
    response: reqwest::Response,
) -> Result<serde_json::Value, McpServiceError> {
    let header = response
        .headers()
        .get("payment-required")
        .ok_or_else(|| {
            McpServiceError::OrchestratorError(
                "402 response missing Payment-Required header (expected x402 v2 shape)".to_string(),
            )
        })?
        .to_str()
        .map_err(|e| {
            McpServiceError::OrchestratorError(format!(
                "Payment-Required header is not valid UTF-8: {e}"
            ))
        })?
        .to_string();
    debug_assert!(
        !header.is_empty(),
        "Payment-Required header must be non-empty"
    );

    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&header)
        .map_err(|e| {
            McpServiceError::OrchestratorError(format!(
                "Payment-Required header is not valid base64: {e}"
            ))
        })?;
    let payment_details: serde_json::Value = serde_json::from_slice(&decoded).map_err(|e| {
        McpServiceError::OrchestratorError(format!("Payment-Required body is not valid JSON: {e}"))
    })?;
    // Postcondition: the returned envelope always carries the payment_required marker.
    Ok(serde_json::json!({
        "status": "payment_required",
        "instructions": "Pay 5 USDC to the address below, then call generate_video again with your tx_signature (a base64-encoded x402 v2 PaymentPayload).",
        "payment_details": payment_details,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_summary_parses_orchestrator_wire_format() {
        // This is a real (trimmed) response from
        // shillbot-api GET /tasks. Regression guard against the
        // proxy's TaskSummary drifting away from the orchestrator's
        // TaskResponse shape — that mismatch caused the
        // "error decoding response body" bug in the live MCP server.
        let json = serde_json::json!({
            "task_id": "campaign-uuid:task-uuid",
            "campaign_id": "campaign-uuid",
            "campaign_topic": "Play a round of coordination.game",
            "state": "open",
            "agent": null,
            "content_id": null,
            "composite_score": null,
            "payment_amount": null,
            "platform": 5,
            "created_at": "2026-04-07T08:20:57Z",
            "claimed_at": null,
            "submitted_at": null,
            "brief": {
                "topic": "Play a round of coordination.game",
                "brand_voice": "Direct incentive.",
                "cta": "Play one round at coordination.game",
                "utm_link": "https://coordination.game",
                "blocklist": [],
                "examples": []
            },
            "estimated_payment_lamports": 20_000_000u64,
            "quality_threshold": 200_000u64,
            "scoring_scales": { "views_scale": 5000, "likes_scale": 250, "comments_scale": 50 }
        });

        let parsed: TaskSummary = serde_json::from_value(json).expect("must deserialize");
        assert_eq!(parsed.task_id, "campaign-uuid:task-uuid");
        assert_eq!(parsed.state, "open");
        assert_eq!(parsed.platform, Some(5));
        assert_eq!(parsed.estimated_payment_lamports, Some(20_000_000));
        assert_eq!(
            parsed.campaign_topic.as_deref(),
            Some("Play a round of coordination.game")
        );
        assert!(parsed.brief.is_some());
    }

    #[test]
    fn task_list_response_parses_with_next_cursor_camelcase_or_snake() {
        // Orchestrator currently emits snake_case `next_cursor`. Test the
        // happy path to lock in the contract.
        let json = serde_json::json!({
            "tasks": [
                {
                    "task_id": "c:t",
                    "state": "open",
                    "platform": 3,
                    "quality_threshold": 0
                }
            ],
            "next_cursor": null
        });
        let parsed: TaskListResponse = serde_json::from_value(json).expect("must deserialize");
        assert_eq!(parsed.tasks.len(), 1);
        assert_eq!(parsed.tasks[0].task_id, "c:t");
        assert!(parsed.next_cursor.is_none());
    }

    #[test]
    fn transaction_response_parses_orchestrator_claim_payload() {
        // Trimmed real shape from `POST /tasks/:id/claim` — the orchestrator
        // returns the unsigned tx as base64. The MCP proxy must round-trip
        // this without losing the `transaction` field.
        let json = serde_json::json!({
            "message": "Sign and submit this transaction to claim the task on-chain. Then call POST /tasks/:id/confirm with the tx signature.",
            "task_id": "campaign-uuid:task-uuid",
            "transaction": "AQAAAA...base64-bytes...AAAB"
        });
        let parsed: TransactionResponse = serde_json::from_value(json).expect("must deserialize");
        assert_eq!(parsed.task_id, "campaign-uuid:task-uuid");
        assert_eq!(
            parsed.transaction.as_deref(),
            Some("AQAAAA...base64-bytes...AAAB")
        );
        assert!(parsed.task_pda.is_none());
    }

    #[test]
    fn confirm_action_serializes_snake_case() {
        // Must match shillbot-api's
        // #[serde(rename_all = "snake_case")] on ConfirmAction.
        assert_eq!(
            serde_json::to_value(ConfirmAction::Claim).unwrap(),
            serde_json::Value::String("claim".to_string())
        );
        assert_eq!(
            serde_json::to_value(ConfirmAction::Submit).unwrap(),
            serde_json::Value::String("submit".to_string())
        );
        // approve must serialize as `"approve"` (snake_case) to match the
        // orchestrator enum; a casing drift would silently route confirm
        // calls into serde's fallthrough and break the client review gate.
        assert_eq!(
            serde_json::to_value(ConfirmAction::Approve).unwrap(),
            serde_json::Value::String("approve".to_string())
        );
        assert_eq!(
            serde_json::to_value(ConfirmAction::Verify).unwrap(),
            serde_json::Value::String("verify".to_string())
        );
        assert_eq!(
            serde_json::to_value(ConfirmAction::Finalize).unwrap(),
            serde_json::Value::String("finalize".to_string())
        );
    }

    #[test]
    fn network_query_suffix_none_is_empty() {
        // Mainnet-default behaviour: omitting the network must produce no
        // query suffix so the orchestrator's existing routes keep working
        // unchanged for clients that don't pass `network`.
        assert_eq!(network_query_suffix(None), "");
    }

    #[test]
    fn network_query_suffix_devnet_is_url_encoded() {
        // Devnet is the primary non-default value; the orchestrator
        // dispatches per-network internally based on this exact token.
        assert_eq!(network_query_suffix(Some("devnet")), "?network=devnet");
    }

    #[test]
    fn network_query_suffix_mainnet_is_explicit_when_passed() {
        // We don't strip mainnet here — let the orchestrator handle it.
        // This keeps the proxy honest: what came in goes out, modulo
        // url-encoding.
        assert_eq!(network_query_suffix(Some("mainnet")), "?network=mainnet");
    }

    #[test]
    fn network_query_suffix_url_encodes_unexpected_chars() {
        // The handler validates the token before reaching the proxy, but
        // belt-and-suspenders: anything weird that does sneak through is
        // url-encoded so it can't break the URL parse on the orchestrator
        // side. `=` -> `%3D`.
        let suffix = network_query_suffix(Some("a=b"));
        assert_eq!(suffix, "?network=a%3Db");
    }

    #[test]
    fn test_earnings_response_serialization() {
        let earnings = EarningsResponse {
            total_earned_lamports: 10_000_000,
            total_subsidy_lamports: 1_000_000,
            tasks_completed: 5,
            average_score: 0.75,
        };

        let json = serde_json::to_string(&earnings).unwrap();
        let parsed: EarningsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_earned_lamports, 10_000_000);
        assert_eq!(parsed.total_subsidy_lamports, 1_000_000);
        assert_eq!(parsed.tasks_completed, 5);
        assert_eq!(parsed.average_score, 0.75);
    }

    /// Flow tests: every Shillbot proxy method must forward
    /// `network = Some("devnet")` as a `?network=devnet` query string. The
    /// orchestrator already dispatches per-network internally based on this
    /// exact token; the regression risk we're guarding against is the proxy
    /// silently dropping the parameter and pinning every call to mainnet.
    ///
    /// We use `wiremock` to stand up a fake orchestrator that asserts the
    /// query string the proxy emits. Each method gets its own happy-path
    /// devnet flow so a failure points at the offending method without
    /// fishing through a single combined assertion.
    mod network_flow_tests {
        use super::*;
        use wiremock::matchers::{header, method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        /// Minimal `TaskSummary`-shaped JSON the proxy can deserialize.
        fn minimal_task_json(task_id: &str, state: &str) -> serde_json::Value {
            serde_json::json!({
                "task_id": task_id,
                "state": state,
                "platform": 5,
                "quality_threshold": 0,
            })
        }

        /// Minimal `TransactionResponse`-shaped JSON.
        fn minimal_tx_json(task_id: &str) -> serde_json::Value {
            serde_json::json!({
                "message": "ok",
                "task_id": task_id,
                "transaction": "AAAA",
            })
        }

        #[tokio::test]
        async fn list_tasks_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/tasks"))
                .and(query_param("network", "devnet"))
                .and(query_param("limit", "20"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "tasks": [],
                    "next_cursor": null,
                })))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            let result = proxy
                .list_tasks(None, None, Some("devnet"))
                .await
                .expect("ok");
            assert!(result.tasks.is_empty());
        }

        #[tokio::test]
        async fn list_tasks_no_network_omits_query_param() {
            // Mainnet-default behaviour: the orchestrator's default route
            // must keep working unchanged when network is None.
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/tasks"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "tasks": [],
                    "next_cursor": null,
                })))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            // Verify the request URL had no `network` param. Wiremock
            // doesn't expose a "missing param" matcher cleanly, so we
            // pull the request log and assert it directly.
            proxy.list_tasks(None, None, None).await.expect("ok");
            let received = server.received_requests().await.expect("requests recorded");
            assert_eq!(received.len(), 1);
            let url = received[0].url.as_str();
            assert!(
                !url.contains("network="),
                "expected no network query param when network=None, got url={url}"
            );
        }

        #[tokio::test]
        async fn get_task_details_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/tasks/c:t"))
                .and(query_param("network", "devnet"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(minimal_task_json("c:t", "open")),
                )
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            let task = proxy
                .get_task_details("c:t", Some("devnet"))
                .await
                .expect("ok");
            assert_eq!(task.task_id, "c:t");
        }

        #[tokio::test]
        async fn claim_task_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/tasks/c:t/claim"))
                .and(query_param("network", "devnet"))
                .and(header("authorization", "Bearer wallet1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            let resp = proxy
                .claim_task("c:t", "wallet1", Some("devnet"))
                .await
                .expect("ok");
            assert_eq!(resp.task_id, "c:t");
        }

        #[tokio::test]
        async fn submit_task_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/tasks/c:t/submit"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            let resp = proxy
                .submit_task("c:t", "wallet1", "yt-abc", Some("devnet"))
                .await
                .expect("ok");
            assert_eq!(resp.task_id, "c:t");
        }

        #[tokio::test]
        async fn get_verification_data_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/tasks/c:t/build-verify"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "task_id": "c:t",
                    "task_pda": "PDA111",
                    "composite_score": 750_000,
                    "verification_hash": "deadbeef",
                    "global_state": "GS",
                    "switchboard_feed": "FEED",
                })))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            let resp = proxy
                .get_verification_data("c:t", "wallet1", Some("devnet"))
                .await
                .expect("ok");
            assert_eq!(resp.task_id, "c:t");
            assert_eq!(resp.composite_score, 750_000);
        }

        #[tokio::test]
        async fn build_finalize_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/tasks/c:t/build-finalize"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            proxy
                .build_finalize("c:t", "wallet1", Some("devnet"))
                .await
                .expect("ok");
        }

        #[tokio::test]
        async fn approve_task_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/tasks/c:t/approve"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            proxy
                .approve_task("c:t", "wallet1", Some("devnet"))
                .await
                .expect("ok");
        }

        #[tokio::test]
        async fn list_pending_approval_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/client/pending-approval"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "tasks": [],
                    "next_cursor": null,
                })))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            let resp = proxy
                .list_pending_approval("wallet1", Some("devnet"))
                .await
                .expect("ok");
            assert!(resp.tasks.is_empty());
        }

        #[tokio::test]
        async fn confirm_task_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/tasks/c:t/confirm"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "task_id": "c:t",
                    "action": "claim",
                    "message": "ok",
                })))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            proxy
                .confirm_task(
                    "c:t",
                    "wallet1",
                    "sig",
                    ConfirmAction::Claim,
                    Some("devnet"),
                )
                .await
                .expect("ok");
        }

        #[tokio::test]
        async fn get_earnings_forwards_devnet_query() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/agent/earnings"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "total_earned_lamports": 0u64,
                    "total_subsidy_lamports": 0u64,
                    "tasks_completed": 0u64,
                    "average_score": 0.0,
                })))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            proxy
                .get_earnings("wallet1", Some("devnet"))
                .await
                .expect("ok");
        }

        /// End-to-end flow test (the project standard's "flow tests with
        /// mocked I/O"): claim_task -> submit_task -> get_verification_data
        /// -> build_finalize, all with `network = "devnet"`. Asserts each
        /// of the four mocks is hit exactly once, with the network query
        /// string present on every call. Catches a regression where any
        /// single method drops the param mid-pipeline.
        #[tokio::test]
        async fn earn_lifecycle_flow_threads_devnet_through_every_call() {
            let server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(path("/tasks/c:t/claim"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/tasks/c:t/submit"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/tasks/c:t/build-verify"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "task_id": "c:t",
                    "task_pda": "PDA111",
                    "composite_score": 1u64,
                    "verification_hash": "h",
                    "global_state": "g",
                    "switchboard_feed": "f",
                })))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/tasks/c:t/build-finalize"))
                .and(query_param("network", "devnet"))
                .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tx_json("c:t")))
                .expect(1)
                .mount(&server)
                .await;

            let proxy = OrchestratorProxy::new(server.uri(), server.uri());
            proxy
                .claim_task("c:t", "wallet1", Some("devnet"))
                .await
                .expect("claim ok");
            proxy
                .submit_task("c:t", "wallet1", "yt-id", Some("devnet"))
                .await
                .expect("submit ok");
            proxy
                .get_verification_data("c:t", "wallet1", Some("devnet"))
                .await
                .expect("verify-data ok");
            proxy
                .build_finalize("c:t", "wallet1", Some("devnet"))
                .await
                .expect("finalize ok");

            // The `expect(1)` clauses on each Mock above are checked at
            // server drop. Make the assertion explicit anyway by counting
            // the recorded requests — four calls, four hits.
            let requests = server.received_requests().await.expect("requests recorded");
            assert_eq!(requests.len(), 4, "expected exactly four orchestrator hits");
            for req in requests {
                assert!(
                    req.url.query().unwrap_or("").contains("network=devnet"),
                    "request {} missing network=devnet query: {}",
                    req.url.path(),
                    req.url
                );
            }
        }
    }
}
