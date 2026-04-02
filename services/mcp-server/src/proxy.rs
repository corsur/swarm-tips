use crate::errors::McpServiceError;
use serde::{Deserialize, Serialize};

const MAX_TASK_LIMIT: u32 = 100;

/// HTTP client for proxying read operations to the orchestrator API.
pub struct OrchestratorProxy {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub topic: String,
    pub price: u64,
    pub deadline: String,
    pub brief_summary: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskDetails {
    pub brief: String,
    pub blocklist: Vec<String>,
    pub utm_link: String,
    pub cta: String,
    pub nonce: String,
    pub deadline: String,
}

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

        if details.nonce.is_empty() {
            return Err(McpServiceError::OrchestratorError(
                "task nonce must not be empty".to_string(),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_summary_serialization() {
        let task = TaskSummary {
            id: "task_001".to_string(),
            topic: "crypto trading".to_string(),
            price: 500_000,
            deadline: "2026-04-01T00:00:00Z".to_string(),
            brief_summary: "Create a Short about...".to_string(),
        };

        let json = serde_json::to_string(&task).unwrap();
        let parsed: TaskSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "task_001");
        assert_eq!(parsed.price, 500_000);
    }

    #[test]
    fn test_task_details_serialization() {
        let details = TaskDetails {
            brief: "Create a YouTube Short...".to_string(),
            blocklist: vec!["competitor".to_string()],
            utm_link: "https://example.com?utm=test".to_string(),
            cta: "Check out coordination.game".to_string(),
            nonce: "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8".to_string(),
            deadline: "2026-04-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&details).unwrap();
        let parsed: TaskDetails = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.nonce, "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8");
        assert_eq!(parsed.blocklist.len(), 1);
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
