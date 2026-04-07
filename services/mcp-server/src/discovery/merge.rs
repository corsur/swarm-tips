//! Merge `RawServer` records from multiple upstream sources into deduped
//! `EnrichedServer` records. Dedupe key: canonical name (case-insensitive),
//! falling back to repo URL when name is missing.
//!
//! Phase 1 only sees the official registry so this layer is mostly a pass-
//! through, but the structure is in place for Phase 2 when we add best-of-mcp
//! and PulseMCP.

use crate::discovery::classify::classify_layer1;
use crate::discovery::models::{EnrichedServer, RawServer};
use chrono::Utc;
use std::collections::HashMap;

/// Merge raw servers from any number of sources into deduped + classified
/// enriched records. The order of `sources_in_order` matters for tie-breaks:
/// metadata from earlier sources wins (so put the canonical source first).
pub fn merge_and_classify(sources_in_order: Vec<Vec<RawServer>>) -> Vec<EnrichedServer> {
    let now = Utc::now();
    let mut by_key: HashMap<String, EnrichedServer> = HashMap::new();
    let mut total_input = 0usize;

    for source_batch in sources_in_order {
        for raw in source_batch {
            total_input = total_input.saturating_add(1);
            let key = canonical_key(&raw);
            if key.is_empty() {
                continue;
            }

            by_key
                .entry(key)
                .and_modify(|existing| union_into(existing, &raw, now))
                .or_insert_with(|| build_enriched(&raw, now));
        }
    }

    let merged: Vec<EnrichedServer> = by_key.into_values().collect();
    tracing::info!(
        input_count = total_input,
        merged_count = merged.len(),
        "merge_and_classify complete"
    );
    merged
}

/// Canonical dedupe key. Lowercase the name; fall back to lowercased repo URL.
fn canonical_key(raw: &RawServer) -> String {
    if !raw.name.is_empty() {
        return raw.name.to_lowercase();
    }
    raw.github_repo
        .as_deref()
        .map(str::to_lowercase)
        .unwrap_or_default()
}

/// Build a fresh EnrichedServer from a RawServer (first time we see it).
/// Runs Layer 1 classification immediately so the record is queryable.
fn build_enriched(raw: &RawServer, now: chrono::DateTime<Utc>) -> EnrichedServer {
    let classification = classify_layer1(raw);
    EnrichedServer {
        name: raw.name.clone(),
        title: raw.title.clone(),
        description: raw.description.clone(),
        endpoint: raw.endpoint.clone(),
        transport: raw.transport.clone(),
        npm_package: raw.npm_package.clone(),
        github_repo: raw.github_repo.clone(),
        sources: vec![raw.source.clone()],
        source_count: 1,
        upstream_quality_score: raw.upstream_quality_score,
        upstream_visitors_estimate: raw.upstream_visitors_estimate,
        classification,
        layer2_classification: None,
        first_seen_at: now,
        last_seen_at: now,
    }
}

/// Add a second observation of an existing server. Union the metadata fields,
/// taking non-None values from the new observation only when the existing
/// record is missing them. Always bumps last_seen_at.
fn union_into(existing: &mut EnrichedServer, raw: &RawServer, now: chrono::DateTime<Utc>) {
    if existing.title.is_none() {
        existing.title = raw.title.clone();
    }
    if existing.description.is_none() {
        existing.description = raw.description.clone();
    }
    if existing.endpoint.is_none() {
        existing.endpoint = raw.endpoint.clone();
    }
    if existing.transport.is_none() {
        existing.transport = raw.transport.clone();
    }
    if existing.npm_package.is_none() {
        existing.npm_package = raw.npm_package.clone();
    }
    if existing.github_repo.is_none() {
        existing.github_repo = raw.github_repo.clone();
    }
    if existing.upstream_quality_score.is_none() {
        existing.upstream_quality_score = raw.upstream_quality_score;
    }
    if existing.upstream_visitors_estimate.is_none() {
        existing.upstream_visitors_estimate = raw.upstream_visitors_estimate;
    }
    if !existing.sources.contains(&raw.source) {
        existing.sources.push(raw.source.clone());
        existing.source_count = existing.source_count.saturating_add(1);
    }
    existing.last_seen_at = now;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(name: &str, source: &str) -> RawServer {
        RawServer {
            name: name.to_string(),
            title: None,
            description: Some("desc".to_string()),
            endpoint: None,
            transport: None,
            npm_package: None,
            github_repo: None,
            source: source.to_string(),
            upstream_quality_score: None,
            upstream_visitors_estimate: None,
        }
    }

    #[test]
    fn merge_dedupes_by_name_case_insensitive() {
        let merged = merge_and_classify(vec![vec![
            raw("io.github.foo/bar", "official"),
            raw("io.github.FOO/BAR", "best_of"),
        ]]);
        assert_eq!(merged.len(), 1, "case variants should dedupe");
        let entry = &merged[0];
        assert_eq!(entry.source_count, 2);
        assert!(entry.sources.contains(&"official".to_string()));
        assert!(entry.sources.contains(&"best_of".to_string()));
    }

    #[test]
    fn merge_unions_metadata_from_multiple_sources() {
        let mut a = raw("io.github.foo/bar", "official");
        a.endpoint = Some("https://a.example/mcp".to_string());

        let mut b = raw("io.github.foo/bar", "best_of");
        b.title = Some("Foo Bar Server".to_string());
        b.upstream_quality_score = Some(7.5);

        let merged = merge_and_classify(vec![vec![a], vec![b]]);
        assert_eq!(merged.len(), 1);
        let entry = &merged[0];
        assert_eq!(entry.endpoint.as_deref(), Some("https://a.example/mcp"));
        assert_eq!(entry.title.as_deref(), Some("Foo Bar Server"));
        assert_eq!(entry.upstream_quality_score, Some(7.5));
        assert_eq!(entry.source_count, 2);
    }

    #[test]
    fn merge_drops_unkeyable_records() {
        let mut bad = raw("", "official");
        bad.github_repo = None;
        let merged = merge_and_classify(vec![vec![bad]]);
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_falls_back_to_repo_url_when_name_missing() {
        let mut a = raw("", "official");
        a.github_repo = Some("https://github.com/foo/bar".to_string());

        let mut b = raw("", "best_of");
        b.github_repo = Some("https://github.com/FOO/BAR".to_string());

        let merged = merge_and_classify(vec![vec![a], vec![b]]);
        assert_eq!(
            merged.len(),
            1,
            "should dedupe by repo URL when name is empty"
        );
    }

    #[test]
    fn merge_runs_classification_layer_1() {
        let mut bounty = raw("io.github.example/bounty-server", "official");
        bounty.description =
            Some("MCP server exposing a bounty marketplace for autonomous agents".to_string());
        let merged = merge_and_classify(vec![vec![bounty]]);
        assert_eq!(merged.len(), 1);
        // Layer 1 should fire on the "bounty" keyword + "agents"
        assert!(
            merged[0].classification.confident,
            "Layer 1 should classify bounty servers confidently"
        );
    }
}
