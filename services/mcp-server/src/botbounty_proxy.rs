use crate::errors::McpServiceError;
use serde::{Deserialize, Serialize};

/// HTTP client for proxying operations to the BotBounty API (Base / ETH).
pub struct BotBountyProxy {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BotBountyBounty {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub amount: f64,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, rename = "acceptanceCriteria")]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub poster: String,
    #[serde(default)]
    pub solver: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BotBountyBountyList {
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub bounties: Vec<BotBountyBounty>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BotBountyClaimResponse {
    pub success: Option<bool>,
    pub message: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BotBountySubmitResponse {
    pub success: Option<bool>,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl BotBountyProxy {
    pub fn new(base_url: String) -> Self {
        let url = if base_url.is_empty() {
            "https://botbounty-production.up.railway.app/api".to_string()
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
        category: Option<&str>,
    ) -> Result<Vec<BotBountyBounty>, McpServiceError> {
        let mut url = format!("{}/agent/bounties", self.base_url);
        let mut params = Vec::new();
        if let Some(l) = limit {
            params.push(format!("limit={l}"));
        }
        if let Some(c) = category {
            params.push(format!("category={c}"));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let res = self
            .client
            .get(&url)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| McpServiceError::External(format!("BotBounty list failed: {e}")))?;

        if !res.status().is_success() {
            return Err(McpServiceError::External(format!(
                "BotBounty returned {}",
                res.status()
            )));
        }

        let data: BotBountyBountyList = res
            .json()
            .await
            .map_err(|e| McpServiceError::External(format!("BotBounty parse failed: {e}")))?;

        Ok(data.bounties)
    }

    pub async fn get_bounty(&self, bounty_id: &str) -> Result<BotBountyBounty, McpServiceError> {
        let url = format!("{}/agent/bounties/{bounty_id}", self.base_url);

        let res = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| McpServiceError::External(format!("BotBounty get failed: {e}")))?;

        if !res.status().is_success() {
            return Err(McpServiceError::External(format!(
                "BotBounty bounty {} returned {}",
                bounty_id,
                res.status()
            )));
        }

        res.json()
            .await
            .map_err(|e| McpServiceError::External(format!("BotBounty parse failed: {e}")))
    }

    pub async fn claim_bounty(
        &self,
        bounty_id: &str,
        wallet_address: &str,
        agent_name: &str,
    ) -> Result<BotBountyClaimResponse, McpServiceError> {
        let url = format!("{}/agent/bounties/{bounty_id}/claim", self.base_url);

        let res = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "walletAddress": wallet_address,
                "agentName": agent_name,
            }))
            .send()
            .await
            .map_err(|e| McpServiceError::External(format!("BotBounty claim failed: {e}")))?;

        res.json()
            .await
            .map_err(|e| McpServiceError::External(format!("BotBounty parse failed: {e}")))
    }

    pub async fn submit_work(
        &self,
        bounty_id: &str,
        wallet_address: &str,
        deliverables: &[serde_json::Value],
        notes: Option<&str>,
    ) -> Result<BotBountySubmitResponse, McpServiceError> {
        let url = format!("{}/agent/bounties/{bounty_id}/submit", self.base_url);

        let mut body = serde_json::json!({
            "walletAddress": wallet_address,
            "deliverables": deliverables,
        });
        if let Some(n) = notes {
            body["notes"] = serde_json::Value::String(n.to_string());
        }

        let res = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| McpServiceError::External(format!("BotBounty submit failed: {e}")))?;

        res.json()
            .await
            .map_err(|e| McpServiceError::External(format!("BotBounty parse failed: {e}")))
    }
}
