//! Layer 3 — curated MCP server directory.
//!
//! 30 entries across three vetting tiers (first-party / vetted /
//! discovered) at compile-time-baked into the binary via
//! `include_str!`. The same JSON file lives at
//! `frontend/swarm-tips/src/data/layer3-mcp-servers.json` in
//! `coordination-app` and powers the `/discover` Astro page (#25);
//! when that file changes, copy the new version here and rebuild.
//!
//! Drift is a real risk — two copies of the same JSON across two
//! repos. v2 candidate: pull from a single canonical URL at runtime
//! (the swarm.tips frontend serves the JSON, or a separate S3-backed
//! `layer3.json` endpoint). For v1 the compile-time-bake is the
//! cheapest way to ship the tool without adding a new HTTP fetcher.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const LAYER3_JSON: &str = include_str!("../data/layer3-mcp-servers.json");

/// One Layer 3 directory entry. Mirrors the JSON shape exactly so
/// serde can deserialize directly. Fields that aren't part of the
/// MCP tool's response (e.g., `$schema_doc`, `tiers`) are not on
/// this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer3Entry {
    pub name: String,
    pub url: String,
    pub category: String,
    pub tier: String,
    pub tags: Vec<String>,
    pub description: String,
    pub why_listed: String,
}

/// The static directory parsed once at first access. Subsequent calls
/// return the cached `&'static Vec<Layer3Entry>`.
pub fn entries() -> &'static Vec<Layer3Entry> {
    static CACHE: OnceLock<Vec<Layer3Entry>> = OnceLock::new();
    CACHE.get_or_init(|| {
        #[derive(Deserialize)]
        struct Wire {
            entries: Vec<Layer3Entry>,
        }
        let parsed: Wire = serde_json::from_str(LAYER3_JSON)
            .expect("layer3-mcp-servers.json must parse at startup");
        parsed.entries
    })
}

/// Filter parameters mirroring the `search_mcp_servers` tool args.
/// Each is an inclusive narrowing — None means "no filter for this
/// dimension."
#[derive(Debug, Default)]
pub struct Filter<'a> {
    pub query: Option<&'a str>,
    pub category: Option<&'a str>,
    pub tier: Option<&'a str>,
    pub limit: Option<usize>,
}

/// Apply a filter to the static directory. Pure logic — no I/O. The
/// MCP tool wraps this with arg parsing + result wrapping.
pub fn search(filter: &Filter) -> Vec<&'static Layer3Entry> {
    let q = filter.query.map(|s| s.to_lowercase());
    let cat = filter.category.map(|s| s.to_lowercase());
    let tier = filter.tier.map(|s| s.to_lowercase());

    let mut out: Vec<&Layer3Entry> = entries()
        .iter()
        .filter(|e| {
            if let Some(t) = tier.as_deref() {
                if e.tier.to_lowercase() != t {
                    return false;
                }
            }
            if let Some(c) = cat.as_deref() {
                if !e.category.to_lowercase().contains(c) {
                    return false;
                }
            }
            if let Some(needle) = q.as_deref() {
                if !entry_matches_query(e, needle) {
                    return false;
                }
            }
            true
        })
        .collect();

    // Tier ordering: first-party → vetted → discovered. Entries the
    // DAO recommends lead the result list; discovered entries follow.
    out.sort_by_key(|e| match e.tier.as_str() {
        "first-party" => 0,
        "vetted" => 1,
        "discovered" => 2,
        _ => 99,
    });

    if let Some(limit) = filter.limit {
        out.truncate(limit);
    }
    out
}

/// Substring match against name, description, OR any tag (case-
/// insensitive). The `needle` is already lowercased by the caller.
fn entry_matches_query(entry: &Layer3Entry, needle: &str) -> bool {
    if entry.name.to_lowercase().contains(needle) {
        return true;
    }
    if entry.description.to_lowercase().contains(needle) {
        return true;
    }
    if entry.why_listed.to_lowercase().contains(needle) {
        return true;
    }
    entry.tags.iter().any(|t| t.to_lowercase().contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_parses_at_build_time() {
        let all = entries();
        assert!(
            all.len() >= 20,
            "Layer 3 directory must have ≥ 20 entries (current: {})",
            all.len()
        );
        // Required fields aren't empty on any entry.
        for e in all {
            assert!(!e.name.is_empty(), "entry name empty");
            assert!(!e.url.is_empty(), "entry url empty");
            assert!(!e.tier.is_empty(), "entry tier empty for {}", e.name);
            assert!(
                !e.category.is_empty(),
                "entry category empty for {}",
                e.name
            );
        }
    }

    #[test]
    fn tier_set_is_closed() {
        // Every entry's tier must be one of the three published tiers.
        // A new tier value would silently slip past the search-sort if
        // we didn't enforce this.
        for e in entries() {
            assert!(
                matches!(e.tier.as_str(), "first-party" | "vetted" | "discovered"),
                "unexpected tier {} on entry {}",
                e.tier,
                e.name
            );
        }
    }

    #[test]
    fn filter_by_tier() {
        let first_party = search(&Filter {
            tier: Some("first-party"),
            ..Default::default()
        });
        for e in &first_party {
            assert_eq!(e.tier, "first-party");
        }
        // 3 first-party entries (Swarm Tips MCP / Coordination Game / Shillbot).
        assert!(
            first_party.len() >= 3,
            "expected ≥ 3 first-party entries, got {}",
            first_party.len()
        );
    }

    #[test]
    fn filter_by_query_matches_name_or_description_or_tags() {
        // "shillbot" appears in multiple entries (Swarm Tips MCP
        // description, Shillbot itself, scorer references). Match
        // against any of the three search fields.
        let hits = search(&Filter {
            query: Some("shillbot"),
            ..Default::default()
        });
        assert!(!hits.is_empty(), "expected ≥ 1 'shillbot' hit");
    }

    #[test]
    fn filter_by_category_substring_match() {
        // "agent" is a substring of multiple categories (agent-bounties,
        // agent-tools-directory, agent-identity, etc.).
        let hits = search(&Filter {
            category: Some("agent"),
            ..Default::default()
        });
        for e in &hits {
            assert!(e.category.contains("agent"));
        }
        assert!(hits.len() >= 2, "expected ≥ 2 'agent' category hits");
    }

    #[test]
    fn limit_is_applied() {
        let hits = search(&Filter {
            limit: Some(3),
            ..Default::default()
        });
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn results_sort_first_party_first() {
        let hits = search(&Filter {
            limit: Some(5),
            ..Default::default()
        });
        // First entry in any result set must be first-party (3 of them
        // exist; with limit=5 they all land first).
        assert_eq!(hits[0].tier, "first-party");
    }

    #[test]
    fn unknown_tier_returns_empty_not_error() {
        let hits = search(&Filter {
            tier: Some("nonexistent-tier"),
            ..Default::default()
        });
        assert!(hits.is_empty());
    }
}
