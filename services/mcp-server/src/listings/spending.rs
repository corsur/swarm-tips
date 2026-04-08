//! Spending opportunities — paid services swarm.tips agents can spend on.
//!
//! v1 hardcodes the swarm.tips first-party spend operations (`generate_video`).
//! As we discover external spend opportunities (Chutes inference, x402-paywalled
//! directory entries, etc.), they get added either as new `fetch_*_spending`
//! sources or by extending the hardcoded list.
//!
//! This is the data source behind the `list_spending_opportunities` MCP tool.

use schemars::JsonSchema;
use serde::Serialize;

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

/// Hardcoded v1 list of first-party paid services. Add to this list as
/// new spend opportunities ship. External spend discovery (Chutes inference,
/// x402-paywalled APIs) is a follow-up plan.
pub fn first_party_spending_opportunities() -> Vec<SpendingOpportunity> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_party_spending_opportunities_includes_generate_video() {
        let ops = first_party_spending_opportunities();
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
}
