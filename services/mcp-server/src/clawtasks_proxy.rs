use crate::errors::McpServiceError;
use serde::{Deserialize, Serialize};

/// HTTP client for proxying operations to the ClawTasks API (Base / USDC).
pub struct ClawTasksProxy {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClawTasksBounty {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub amount: f64,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub deadline_hours: Option<f64>,
    #[serde(default)]
    pub poster: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClawTasksBountyList {
    #[serde(default)]
    pub bounties: Vec<ClawTasksBounty>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClawTasksRegisterResponse {
    pub api_key: Option<String>,
    pub verification_code: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClawTasksClaimResponse {
    pub success: Option<bool>,
    pub message: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClawTasksSubmitResponse {
    pub success: Option<bool>,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl ClawTasksProxy {
    pub fn new(base_url: String) -> Self {
        let url = if base_url.is_empty() {
            "https://clawtasks.com/api".to_string()
        } else {
            base_url
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            client,
            base_url: url,
        }
    }

    pub async fn list_bounties(
        &self,
        limit: Option<u32>,
        tags: Option<&str>,
    ) -> Result<Vec<ClawTasksBounty>, McpServiceError> {
        let mut url = format!("{}/bounties?status=open&sort=recent", self.base_url);
        if let Some(l) = limit {
            url.push_str(&format!("&limit={l}"));
        }
        if let Some(t) = tags {
            url.push_str(&format!("&tags={t}"));
        }

        let res = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks list failed: {e}")))?;

        if !res.status().is_success() {
            return Err(McpServiceError::External(format!(
                "ClawTasks returned {}",
                res.status()
            )));
        }

        let data: serde_json::Value = res
            .json()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks parse failed: {e}")))?;

        let bounties: Vec<ClawTasksBounty> = if let Some(arr) = data.as_array() {
            serde_json::from_value(serde_json::Value::Array(arr.clone())).unwrap_or_default()
        } else if let Some(obj) = data.get("bounties") {
            serde_json::from_value(obj.clone()).unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(bounties)
    }

    pub async fn get_bounty(&self, bounty_id: &str) -> Result<ClawTasksBounty, McpServiceError> {
        let url = format!("{}/bounties/{bounty_id}", self.base_url);

        let res = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks get failed: {e}")))?;

        if !res.status().is_success() {
            return Err(McpServiceError::External(format!(
                "ClawTasks bounty {} returned {}",
                bounty_id,
                res.status()
            )));
        }

        res.json()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks parse failed: {e}")))
    }

    pub async fn register_agent(
        &self,
        name: &str,
        wallet_address: &str,
    ) -> Result<ClawTasksRegisterResponse, McpServiceError> {
        let url = format!("{}/agents", self.base_url);

        let res = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "name": name,
                "wallet_address": wallet_address,
            }))
            .send()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks register failed: {e}")))?;

        res.json()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks parse failed: {e}")))
    }

    pub async fn claim_bounty(
        &self,
        bounty_id: &str,
        api_key: &str,
    ) -> Result<ClawTasksClaimResponse, McpServiceError> {
        let url = format!("{}/bounties/{bounty_id}/claim", self.base_url);

        let res = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks claim failed: {e}")))?;

        res.json()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks parse failed: {e}")))
    }

    pub async fn submit_work(
        &self,
        bounty_id: &str,
        api_key: &str,
        content: &str,
    ) -> Result<ClawTasksSubmitResponse, McpServiceError> {
        let url = format!("{}/bounties/{bounty_id}/submit", self.base_url);

        let res = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks submit failed: {e}")))?;

        res.json()
            .await
            .map_err(|e| McpServiceError::External(format!("ClawTasks parse failed: {e}")))
    }
}
