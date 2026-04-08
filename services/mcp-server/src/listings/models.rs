use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Collection name constants.
pub const LISTINGS: &str = "listings";
pub const LISTING_EVENTS: &str = "listing_events";
pub const SOURCE_HEALTH: &str = "source_health";
pub const INGESTION_CONFIG: &str = "ingestion_config";

/// A listing persisted in Firestore. Document ID: `{source}:{source_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListingDoc {
    pub source: String,
    pub source_id: String,
    pub source_url: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub reward_amount: String,
    pub reward_token: String,
    pub reward_chain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_usd_estimate: Option<f64>,
    pub payment_model: String,
    pub escrow: bool,
    pub posted_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Utc>>,
    pub status: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disappeared_at: Option<DateTime<Utc>>,
    pub filtered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_reason: Option<String>,
}

impl ListingDoc {
    pub fn doc_id(&self) -> String {
        format!("{}:{}", self.source, self.source_id)
    }
}

/// A raw listing fetched from an external source (before persistence).
#[derive(Debug, Clone)]
pub struct RawListing {
    pub source: String,
    pub source_id: String,
    pub source_url: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub reward_amount: String,
    pub reward_token: String,
    pub reward_chain: String,
    pub reward_usd_estimate: Option<f64>,
    pub payment_model: String,
    pub escrow: bool,
    pub posted_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
}

impl RawListing {
    pub fn doc_id(&self) -> String {
        format!("{}:{}", self.source, self.source_id)
    }
}

/// Append-only lifecycle event for a listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListingEventDoc {
    pub listing_id: String,
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// A single health check record within a source_health document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub timestamp: DateTime<Utc>,
    pub status_code: u16,
    pub response_ms: u64,
    pub listing_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Daily health record per source. Document ID: `{source}:{date}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceHealthDoc {
    pub source: String,
    pub date: String,
    pub checks: Vec<HealthCheck>,
    pub total_checks: u32,
    pub successful_checks: u32,
}

/// Configurable ingestion thresholds. Stored in Firestore `ingestion_config/default`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionConfig {
    pub min_reward_usd: f64,
    pub min_description_length: usize,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            min_reward_usd: 1.0,
            min_description_length: 30,
        }
    }
}

/// JSON response format matching the existing AgentJob TypeScript interface.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentJob {
    pub id: String,
    pub source: String,
    pub source_id: String,
    pub source_url: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub reward_amount: String,
    pub reward_token: String,
    pub reward_chain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_usd_estimate: Option<f64>,
    pub payment_model: String,
    pub escrow: bool,
    pub posted_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    pub status: String,
    pub indexed_at: String,
    /// First-party MCP tool name to call when an in-MCP deep integration is
    /// available for this opportunity. Set for `source = "shillbot"` (claim
    /// via `shillbot_claim_task`); `None` for external sources where the
    /// agent navigates to `source_url` off-platform.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub claim_via: Option<String>,
}

impl From<&ListingDoc> for AgentJob {
    fn from(doc: &ListingDoc) -> Self {
        Self {
            id: doc.doc_id(),
            source: doc.source.clone(),
            source_id: doc.source_id.clone(),
            source_url: doc.source_url.clone(),
            title: doc.title.clone(),
            description: doc.description.clone(),
            category: doc.category.clone(),
            tags: doc.tags.clone(),
            reward_amount: doc.reward_amount.clone(),
            reward_token: doc.reward_token.clone(),
            reward_chain: doc.reward_chain.clone(),
            reward_usd_estimate: doc.reward_usd_estimate,
            payment_model: doc.payment_model.clone(),
            escrow: doc.escrow,
            posted_at: doc.posted_at.to_rfc3339(),
            deadline: doc.deadline.map(|d| d.to_rfc3339()),
            status: doc.status.clone(),
            indexed_at: doc.last_seen_at.to_rfc3339(),
            // claim_via is set per-call by the unified `list_earning_opportunities`
            // MCP tool based on `source` — not persisted with the listing.
            claim_via: None,
        }
    }
}
