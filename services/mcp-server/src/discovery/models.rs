//! Discovery module data models.
//!
//! `RawServer` is what we get from each upstream registry (after parsing the
//! source-specific JSON/YAML schema). `EnrichedServer` is the merged +
//! classified record we store in Firestore and serve via the internal API.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Cash-flow direction for a server (mining priority order from the user:
/// (1) earning opportunities, (2) composable primitives, (3) market intelligence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashFlowDirection {
    /// Server lets the calling agent EARN crypto. Top-priority signal for swarm.tips.
    EarnsForAgent,
    /// Server costs the calling agent crypto (paid API, video gen, etc.).
    CostsAgent,
    /// Neither — read-only data, infrastructure, or no money flow.
    Neutral,
}

/// Coarse category buckets. Loose by design — Layer 1 only assigns these when
/// the heuristic is very confident; Layer 2 (LLM) refines the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Bounty,
    Content,
    Payment,
    Infrastructure,
    Game,
    Social,
    Devtools,
    Data,
    Other,
}

/// What this server is to swarm.tips. The single-line "what do we do with this?" verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueToSwarm {
    /// Aggregate this as a new EARN listing on swarm.tips
    AggregateListing,
    /// Surface as a SPEND listing
    SurfaceAsSpend,
    /// Compete with one of our verticals — keep an eye on it
    Competitor,
    /// Complement one of our verticals — possible partnership
    Complement,
    /// Useful primitive other agents could build on (RPC, indexer, etc.)
    Dependency,
    /// Inspiration only — interesting but not actionable
    Inspiration,
    /// No clear value
    None,
}

/// One server as fetched from a single upstream source, before merging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawServer {
    /// Canonical name (e.g. "io.github.corsur/swarm-tips") if known
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub transport: Option<String>,
    pub npm_package: Option<String>,
    pub github_repo: Option<String>,
    /// Which source this record came from (for source-tracking after merge)
    pub source: String,
    /// Pre-computed quality score from upstream, if any (best-of-mcp publishes one)
    pub upstream_quality_score: Option<f32>,
    /// Visitor estimate from upstream, if any (PulseMCP publishes this)
    pub upstream_visitors_estimate: Option<u64>,
}

/// Layer 1 classification result for a server. Pure heuristics, no LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer1Classification {
    pub category: Option<Category>,
    pub cash_flow_direction: Option<CashFlowDirection>,
    /// Currencies the server appears to deal in (SOL, USDC, ETH, etc.)
    pub currencies: Vec<String>,
    pub value_to_swarm: Option<ValueToSwarm>,
    /// Whether ANY heuristic fired confidently — if false, this server needs Layer 2
    pub confident: bool,
    /// Free-form list of which heuristics matched (for debugging)
    pub matched_signals: Vec<String>,
}

/// Final enriched server record stored in Firestore at `mcp_servers/{slug}`.
/// Combines metadata from all sources + Layer 1 classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedServer {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub transport: Option<String>,
    pub npm_package: Option<String>,
    pub github_repo: Option<String>,
    /// Sources where we found this server (e.g. ["official", "best_of_mcp", "pulse_mcp"])
    pub sources: Vec<String>,
    /// Number of sources — easy popularity proxy
    pub source_count: u32,
    pub upstream_quality_score: Option<f32>,
    pub upstream_visitors_estimate: Option<u64>,
    pub classification: Layer1Classification,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

impl EnrichedServer {
    /// Document ID slug, derived from canonical name. Slashes are replaced
    /// because Firestore document IDs can't contain `/`.
    pub fn slug(&self) -> String {
        self.name.replace('/', "__")
    }

    /// True if this server should appear in the earning-candidates list.
    /// Used by the `/internal/mcp/earning-candidates` endpoint.
    pub fn is_earning_candidate(&self) -> bool {
        matches!(
            self.classification.cash_flow_direction,
            Some(CashFlowDirection::EarnsForAgent)
        ) || matches!(
            self.classification.value_to_swarm,
            Some(ValueToSwarm::AggregateListing)
        )
    }
}

/// Firestore collection name for the merged server index.
pub const MCP_SERVERS_COLLECTION: &str = "mcp_servers";
