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
            .header("X-Agent-Wallet", wallet_pubkey)
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

        // If the agent provides a tx_signature, include the x402 payment proof header
        if let Some(sig) = tx_signature {
            req = req.header("X-PAYMENT", sig);
        }

        let response = req.send().await.map_err(|e| {
            tracing::error!(service = "mcp-server", error = %e, "orchestrator create_short_crypto failed");
            McpServiceError::OrchestratorError(format!("request failed: {e}"))
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
