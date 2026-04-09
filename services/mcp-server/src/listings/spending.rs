//! Spending opportunities — paid services swarm.tips agents can spend on.
//!
//! Mirrors the earning side's `fetch_*` parallel-aggregation pattern from
//! `listings/sources.rs`. v1 has one source: `fetch_first_party_spending`,
//! which returns the hardcoded swarm.tips first-party spend operations
//! (currently just `generate_video`). External spend sources (Chutes
//! inference, x402-paywalled directories, Replicate, Hugging Face Spaces)
//! get added as new `fetch_*_spending` functions and wired into the
//! `tokio::join!` in `get_spending_opportunities`. No further structural
//! refactoring required as new sources land.
//!
//! This is the data source behind the `list_spending_opportunities` MCP tool.

use crate::listings::models::HealthCheck;
use chrono::Utc;
use schemars::JsonSchema;
use serde::Serialize;
use std::time::Instant;

/// A paid service an agent can spend on. Mirrors `AgentJob` but inverted for
/// outflow rather than inflow.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SpendingOpportunity {
    pub title: String,
    pub description: String,
    /// Source platform — e.g., `"swarm.tips"`, `"chutes"`, `"x402-..."`.
    pub source: String,
    /// Coarse category — e.g., `"video"`, `"inference"`, `"compute"`.
    pub category: String,
    pub cost_amount: String,
    pub cost_token: String,
    pub cost_chain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd_estimate: Option<f64>,
    /// Direct redirect URL for the agent to act on the opportunity off-platform.
    pub url: String,
    /// First-party tool name to call when the agent has an in-MCP deep
    /// integration available. `None` for external sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spend_via: Option<String>,
}

/// Result of fetching from one spending source: opportunities + health check.
/// Mirrors `FetchResult` from `listings/sources.rs` for shape consistency.
pub struct SpendingFetchResult {
    pub source: String,
    pub opportunities: Vec<SpendingOpportunity>,
    pub health: HealthCheck,
}

/// Hardcoded v1 list of first-party paid services. Add to this list as
/// new first-party spend opportunities ship.
fn first_party_opportunities() -> Vec<SpendingOpportunity> {
    vec![SpendingOpportunity {
        title: "Generate a short-form video".to_string(),
        description: "Create an AI-generated YouTube Short / TikTok / Reel from a prompt or URL. \
             720p 9:16 vertical with narration overlay, ~30 seconds. Pay 5 USDC via the \
             x402 protocol on Base, Ethereum, Polygon, or Solana, then call again with \
             the broadcast tx hash to trigger generation. Generated videos can be \
             submitted to Shillbot tasks via shillbot_submit_work to earn back more \
             than the spend."
            .to_string(),
        source: "swarm.tips".to_string(),
        category: "video".to_string(),
        cost_amount: "5".to_string(),
        cost_token: "USDC".to_string(),
        cost_chain: "base|ethereum|polygon|solana".to_string(),
        cost_usd_estimate: Some(5.0),
        url: "https://shillbot.org".to_string(),
        spend_via: Some("generate_video".to_string()),
    }]
}

/// Fetch first-party swarm.tips spend opportunities. Mirrors the
/// `fetch_*` adapter pattern from `listings/sources.rs` so future external
/// sources can be added with the same shape. v1 wraps the hardcoded list;
/// no actual HTTP traffic. The `_client` parameter is unused but kept for
/// signature consistency with future external `fetch_*_spending` sources.
pub async fn fetch_first_party_spending(_client: &reqwest::Client) -> SpendingFetchResult {
    let start = Instant::now();
    let opportunities = first_party_opportunities();
    let count = opportunities.len() as u32;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    SpendingFetchResult {
        source: "swarm.tips".to_string(),
        opportunities,
        health: HealthCheck {
            timestamp: Utc::now(),
            status_code: 200,
            response_ms: elapsed_ms,
            listing_count: count,
            error: None,
        },
    }
}

/// Aggregate spending opportunities across all sources. Mirrors `get_listings`
/// from `listings/mod.rs`: parallel fetches via `tokio::join!`, dedupe, return
/// the merged vec. v1 has one source (`fetch_first_party_spending`) so the
/// "parallel" is degenerate, but the structure is in place for future external
/// sources to land without further refactoring. Per-source health is logged
/// at INFO so source failures are visible in Cloud Logging.
pub async fn get_spending_opportunities(client: &reqwest::Client) -> Vec<SpendingOpportunity> {
    let (first_party,) = tokio::join!(fetch_first_party_spending(client));

    let fetch_results = vec![first_party];

    // Collect all opportunities, deduping by (source, title) pair.
    // Also log per-source health so failures are visible.
    let mut seen = std::collections::HashSet::new();
    let mut all: Vec<SpendingOpportunity> = Vec::new();
    for result in fetch_results {
        tracing::info!(
            source = %result.source,
            count = result.health.listing_count,
            status_code = result.health.status_code,
            response_ms = result.health.response_ms,
            error = result.health.error.as_deref().unwrap_or(""),
            "spending source health"
        );
        for opp in result.opportunities {
            let key = format!("{}:{}", opp.source, opp.title);
            if seen.insert(key) {
                all.push(opp);
            }
        }
    }

    tracing::info!(
        total_fetched = all.len(),
        "fetched spending opportunities from sources"
    );
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_party_opportunities_includes_generate_video() {
        let ops = first_party_opportunities();
        assert_eq!(ops.len(), 1);
        let video = &ops[0];
        assert_eq!(video.source, "swarm.tips");
        assert_eq!(video.category, "video");
        assert_eq!(video.cost_token, "USDC");
        assert_eq!(video.cost_amount, "5");
        assert_eq!(video.cost_usd_estimate, Some(5.0));
        assert_eq!(video.spend_via.as_deref(), Some("generate_video"));
    }

    #[test]
    fn spending_opportunity_serializes_skip_none() {
        let op = SpendingOpportunity {
            title: "test".to_string(),
            description: "test".to_string(),
            source: "external".to_string(),
            category: "test".to_string(),
            cost_amount: "1".to_string(),
            cost_token: "USDC".to_string(),
            cost_chain: "base".to_string(),
            cost_usd_estimate: None,
            url: "https://example.com".to_string(),
            spend_via: None,
        };
        let json = serde_json::to_string(&op).expect("must serialize");
        // Skip-if-none should drop both fields when None
        assert!(!json.contains("cost_usd_estimate"));
        assert!(!json.contains("spend_via"));
    }

    #[tokio::test]
    async fn fetch_first_party_spending_returns_video_with_health() {
        let client = reqwest::Client::new();
        let result = fetch_first_party_spending(&client).await;
        assert_eq!(result.source, "swarm.tips");
        assert_eq!(result.opportunities.len(), 1);
        assert_eq!(result.health.status_code, 200);
        assert_eq!(result.health.listing_count, 1);
        assert!(result.health.error.is_none());
    }

    #[tokio::test]
    async fn get_spending_opportunities_aggregates_first_party() {
        let client = reqwest::Client::new();
        let opps = get_spending_opportunities(&client).await;
        // v1 has only the first-party source, expect 1 entry
        assert_eq!(opps.len(), 1);
        let video = &opps[0];
        assert_eq!(video.source, "swarm.tips");
        assert_eq!(video.spend_via.as_deref(), Some("generate_video"));
    }
}
