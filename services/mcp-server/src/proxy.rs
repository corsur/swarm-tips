use crate::errors::McpServiceError;
use serde::{Deserialize, Serialize};

const MAX_TASK_LIMIT: u32 = 100;

/// HTTP client for proxying read operations to the orchestrator API.
pub struct OrchestratorProxy {
    client: reqwest::Client,
    base_url: String,
}

/// One task as returned by the orchestrator's `GET /tasks` and
/// `GET /tasks/:id` endpoints. Field names mirror the orchestrator's wire
/// format (`shillbot-orchestrator::models::task::TaskResponse`) so serde can
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

#[derive(Debug, Serialize, Deserialize)]
pub struct EarningsResponse {
    pub total_earned: u64,
    pub tasks_completed: u32,
    pub average_score: u64,
    pub pending_tasks: u32,
}

/// Mirrors `shillbot-orchestrator::models::task::TransactionResponse`. Returned
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

/// Action discriminator for `POST /tasks/:id/confirm`. Mirrors
/// `shillbot-orchestrator::models::task::ConfirmAction` — must serialize as
/// snake_case to match the orchestrator's `#[serde(rename_all = "snake_case")]`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmAction {
    Claim,
    Submit,
}

/// Mirrors `shillbot-orchestrator::models::task::ConfirmTaskResponse`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfirmTaskResponse {
    pub task_id: String,
    pub action: String,
    pub message: String,
}

impl OrchestratorProxy {
    pub fn new(base_url: String) -> Self {
        assert!(
            !base_url.is_empty(),
            "orchestrator base_url must not be empty"
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self { client, base_url }
    }

    /// List available tasks from the orchestrator.
    pub async fn list_tasks(
        &self,
        limit: Option<u32>,
        min_price: Option<u64>,
    ) -> Result<TaskListResponse, McpServiceError> {
        let effective_limit = limit.unwrap_or(20).min(MAX_TASK_LIMIT);

        let mut url = format!("{}/tasks?limit={effective_limit}", self.base_url);
        if let Some(price) = min_price {
            url.push_str(&format!("&min_price={price}"));
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
    pub async fn get_task_details(&self, task_id: &str) -> Result<TaskDetails, McpServiceError> {
        if task_id.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "task_id must not be empty".to_string(),
            ));
        }

        let url = format!("{}/tasks/{task_id}", self.base_url);

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
    ) -> Result<EarningsResponse, McpServiceError> {
        if wallet_pubkey.is_empty() {
            return Err(McpServiceError::InvalidInput(
                "wallet_pubkey must not be empty".to_string(),
            ));
        }

        let url = format!("{}/agent/earnings", self.base_url);

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

        if earnings.tasks_completed == 0 && earnings.average_score != 0 {
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

        let url = format!("{}/tasks/{task_id}/claim", self.base_url);
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

        let url = format!("{}/tasks/{task_id}/submit", self.base_url);
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

    /// Notify the orchestrator that a `claim_task` or `submit_work` transaction
    /// landed on-chain. The orchestrator verifies the signature, deduplicates,
    /// and advances the task state in Firestore.
    pub async fn confirm_task(
        &self,
        task_id: &str,
        wallet_pubkey: &str,
        tx_signature: &str,
        action: ConfirmAction,
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

        let url = format!("{}/tasks/{task_id}/confirm", self.base_url);
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
    /// The orchestrator's x402 middleware will return 402 if payment is needed.
    pub async fn create_short_crypto(
        &self,
        prompt: &str,
        url: Option<&str>,
        tx_signature: Option<&str>,
    ) -> Result<serde_json::Value, McpServiceError> {
        let endpoint = format!("{}/shorts/create-crypto", self.base_url);

        let mut body = serde_json::json!({ "prompt": prompt });
        if let Some(u) = url {
            body["url"] = serde_json::Value::String(u.to_string());
        }

        let mut req = self.client.post(&endpoint).json(&body);

        // If the agent provides a tx_signature, include the x402 payment proof header.
        // Log the byte length so we can correlate "builder error" failures against
        // payload size — reqwest 0.12's HeaderValue::from_str rejection collapses to
        // a generic "builder error" Display message that hides the source.
        if let Some(sig) = tx_signature {
            tracing::debug!(
                service = "mcp-server",
                payment_header_len = sig.len(),
                "x402: attaching X-PAYMENT header"
            );
            req = req.header("X-PAYMENT", sig);
        }

        let response = req.send().await.map_err(|e| {
            // The Display impl on reqwest::Error::Builder collapses the underlying
            // cause to literally "builder error" with no source field. Walk the
            // std::error::Error::source chain so we can see WHICH step inside the
            // builder actually failed (header validation, URL parse, body serde,
            // etc.) and log the full Debug representation alongside.
            let mut chain = String::new();
            let mut src: Option<&dyn std::error::Error> = std::error::Error::source(&e);
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
        })?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpServiceError::OrchestratorError(format!("invalid response: {e}")))?;

        if status.as_u16() == 402 {
            // Payment required — return the payment instructions to the agent
            return Ok(serde_json::json!({
                "status": "payment_required",
                "instructions": "Pay 5 USDC to the address below, then call generate_video again with your tx_signature.",
                "payment_details": body,
            }));
        }

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

        let url = format!("{}/shorts/{session_id}", self.base_url);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_summary_parses_orchestrator_wire_format() {
        // This is a real (trimmed) response from
        // shillbot-orchestrator GET /tasks. Regression guard against the
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
        // Must match shillbot-orchestrator's
        // #[serde(rename_all = "snake_case")] on ConfirmAction.
        assert_eq!(
            serde_json::to_value(ConfirmAction::Claim).unwrap(),
            serde_json::Value::String("claim".to_string())
        );
        assert_eq!(
            serde_json::to_value(ConfirmAction::Submit).unwrap(),
            serde_json::Value::String("submit".to_string())
        );
    }

    #[test]
    fn test_earnings_response_serialization() {
        let earnings = EarningsResponse {
            total_earned: 10_000_000,
            tasks_completed: 5,
            average_score: 750_000,
            pending_tasks: 2,
        };

        let json = serde_json::to_string(&earnings).unwrap();
        let parsed: EarningsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_earned, 10_000_000);
        assert_eq!(parsed.tasks_completed, 5);
    }
}
