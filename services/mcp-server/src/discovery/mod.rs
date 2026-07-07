//! MCP discovery + mining engine.
//!
//! Pulls every MCP server from upstream registries (no custom crawler — we
//! consume the existing datasets directly), runs Layer 1 pattern matching to
//! classify them by cash-flow direction, and exposes earning-candidate
//! queries via internal HTTP endpoints.
//!
//! Priority order (locked by user): (1) earning opportunities,
//! (2) composable primitives, (3) market intelligence.

pub mod classify;
pub mod deep_analysis;
pub mod llm_classify;
pub mod merge;
pub mod models;
pub mod search;
#[cfg(test)]
mod search_eval;
pub mod sources;

use crate::discovery::llm_classify::{LlmClassifier, MAX_SERVERS_PER_CYCLE};
use crate::discovery::merge::merge_and_classify;
use crate::discovery::models::{EnrichedServer, MCP_SERVERS_COLLECTION};
use crate::discovery::sources::{
    pull_awesome_mcp, pull_best_of_mcp, pull_official_registry, SOURCE_AWESOME_APPCYPHER,
    SOURCE_AWESOME_WONG2, SOURCE_BEST_OF_MCP,
};
use anyhow::{Context, Result};
use firestore::FirestoreDb;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Cached enriched index. Refreshed by the daily refresh handler; served by
/// the earning-candidates endpoint. In-memory cache is fine — Firestore is
/// the durable store, this is just a request-time accelerator.
pub struct DiscoveryCache {
    pub servers: Vec<EnrichedServer>,
    /// When the cache was last refreshed. Reserved for cache-staleness checks
    /// in Phase 2 (e.g. "if older than 24h, kick a refresh in the background").
    /// Currently unused but populated so the field exists when we need it.
    #[allow(dead_code)]
    pub last_refreshed_at: chrono::DateTime<chrono::Utc>,
}

/// Discovery state attached to the mcp-server router.
pub struct DiscoveryState {
    pub db: FirestoreDb,
    pub http: reqwest::Client,
    pub cache: Mutex<Option<DiscoveryCache>>,
    /// BM25 search index over the cached catalog. Rebuilt inside
    /// `refresh_discovery`, invalidated whenever the cache is, and built
    /// lazily on first search (Firestore-backed) after a cold start.
    /// `Arc` so request handlers can search without holding the lock.
    pub search_index: Mutex<Option<Arc<search::SearchIndex>>>,
    /// Layer 2 classifier — populated only if `xai-api-key` is loaded from
    /// GCP Secret Manager at startup. When None, `/internal/mcp/llm-classify`
    /// returns 503 and `refresh_discovery` skips Layer 2.
    pub llm: Option<LlmClassifier>,
    /// GitHub bearer token for raising rate limits on stargazer / README
    /// fetches in `deep_analysis`. Loaded from `github-token` in GCP Secret
    /// Manager. None falls back to unauthenticated calls (60/hour/IP).
    pub github_token: Option<String>,
}

impl DiscoveryState {
    pub fn new(
        db: FirestoreDb,
        http: reqwest::Client,
        llm: Option<LlmClassifier>,
        github_token: Option<String>,
    ) -> Self {
        Self {
            db,
            http,
            cache: Mutex::new(None),
            search_index: Mutex::new(None),
            llm,
            github_token,
        }
    }
}

/// Get the current search index, building it from the cache (or Firestore
/// on a cold start) if absent. Returns None only when both the cache and
/// Firestore are unavailable — the caller should surface "search
/// unavailable" rather than an empty result set.
pub async fn get_or_build_search_index(
    state: &Arc<DiscoveryState>,
) -> Option<Arc<search::SearchIndex>> {
    {
        let idx = state.search_index.lock().await;
        if let Some(existing) = idx.as_ref() {
            return Some(Arc::clone(existing));
        }
    }

    // Absent — build from the catalog cache, falling back to Firestore.
    let (servers, refreshed_at) = {
        let cache = state.cache.lock().await;
        match cache.as_ref() {
            Some(c) => (c.servers.clone(), c.last_refreshed_at),
            None => (Vec::new(), chrono::Utc::now()),
        }
    };
    let (servers, refreshed_at) = if servers.is_empty() {
        match load_from_firestore(&state.db).await {
            Ok(docs) if !docs.is_empty() => {
                let now = chrono::Utc::now();
                let mut cache = state.cache.lock().await;
                *cache = Some(DiscoveryCache {
                    servers: docs.clone(),
                    last_refreshed_at: now,
                });
                (docs, now)
            }
            Ok(_) => return None,
            Err(e) => {
                tracing::warn!(error = %e, "failed to load catalog for search index");
                return None;
            }
        }
    } else {
        (servers, refreshed_at)
    };

    let built = Arc::new(search::SearchIndex::build(servers, refreshed_at));
    tracing::info!(
        corpus = built.corpus_size(),
        "built BM25 search index over discovery catalog"
    );
    let mut idx = state.search_index.lock().await;
    *idx = Some(Arc::clone(&built));
    Some(built)
}

/// Refresh the discovery index: pull from all sources, merge + classify,
/// write to Firestore, update cache. Phase 1 only pulls the official registry.
///
/// Returns the count of merged servers and the count of earning candidates.
pub async fn refresh_discovery(state: &Arc<DiscoveryState>) -> Result<RefreshSummary> {
    let started = std::time::Instant::now();

    // Pull from all sources in parallel. Each source is best-effort — partial
    // failures degrade to empty vecs with an `error!` log so the rest of the
    // refresh still completes.
    let all_sources = pull_all_discovery_sources(state).await;

    // Merge + classify (synchronous, in-memory). This produces records with
    // layer2_classification = layer3_analysis = None — we then patch them
    // back in from existing Firestore docs below so a fresh refresh doesn't
    // wipe out hours of LLM work.
    let mut merged = merge_and_classify(all_sources);

    // Load the existing index once: used both to preserve Layer 2/3 verdicts
    // and to union in servers the (possibly degraded) source pulls missed.
    let existing_by_slug: std::collections::HashMap<String, EnrichedServer> =
        match load_from_firestore(&state.db).await {
            Ok(docs) => docs.into_iter().map(|d| (d.slug(), d)).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "could not load existing index for preservation/union");
                std::collections::HashMap::new()
            }
        };

    // Preserve Layer 2 + Layer 3 across refresh.
    let (preserved_layer2, preserved_layer3) =
        preserve_layer_classifications(&existing_by_slug, &mut merged);

    // Union: sources are best-effort and the official registry paginates
    // flakily — a partial pull must never SHRINK the catalog (observed
    // 2026-07-07: a degraded refresh replaced 1908 docs with 966 and the
    // live search corpus dropped with it). Servers already known but not
    // re-seen this cycle are retained unchanged; their stale
    // `last_seen_at` remains available for future staleness pruning.
    let retained_unseen = union_with_existing(&mut merged, &existing_by_slug);

    let total = merged.len();
    let earning_count = merged.iter().filter(|s| s.is_earning_candidate()).count();

    // Persist to Firestore (one doc per server). Best-effort: log + continue
    // on individual write failures, but the cache update happens regardless
    // so the in-memory copy is always fresh.
    let (written, write_errors) = persist_servers_to_firestore(&state.db, &merged).await;

    // Update in-memory cache + rebuild the search index from the same
    // snapshot so search never serves a different corpus than the cache.
    {
        let refreshed_at = chrono::Utc::now();
        let rebuilt = Arc::new(search::SearchIndex::build(merged.clone(), refreshed_at));
        let mut cache = state.cache.lock().await;
        *cache = Some(DiscoveryCache {
            servers: merged,
            last_refreshed_at: refreshed_at,
        });
        drop(cache);
        let mut idx = state.search_index.lock().await;
        *idx = Some(rebuilt);
    }

    let elapsed_ms = started.elapsed().as_millis();
    tracing::info!(
        total,
        earning_count,
        preserved_layer2,
        preserved_layer3,
        retained_unseen,
        written,
        write_errors,
        elapsed_ms,
        "refresh_discovery complete"
    );

    Ok(RefreshSummary {
        total,
        earning_count,
        firestore_writes: written,
        firestore_write_errors: write_errors,
        elapsed_ms: elapsed_ms as u64,
    })
}

/// Pull every upstream discovery source in parallel and bundle the successes
/// into one outer vec. Each source is best-effort: failures log at `error!`
/// and degrade to "no entries from this source" without aborting the refresh.
async fn pull_all_discovery_sources(
    state: &Arc<DiscoveryState>,
) -> Vec<Vec<crate::discovery::models::RawServer>> {
    let (official, wong2, appcypher, best_of) = tokio::join!(
        pull_official_registry(&state.http),
        pull_awesome_mcp(
            &state.http,
            SOURCE_AWESOME_WONG2,
            "wong2",
            "awesome-mcp-servers"
        ),
        pull_awesome_mcp(
            &state.http,
            SOURCE_AWESOME_APPCYPHER,
            "appcypher",
            "awesome-mcp-servers"
        ),
        pull_best_of_mcp(&state.http),
    );

    let mut all_sources: Vec<Vec<crate::discovery::models::RawServer>> = Vec::new();
    for (label, result) in [
        ("official", official),
        (SOURCE_AWESOME_WONG2, wong2),
        (SOURCE_AWESOME_APPCYPHER, appcypher),
        (SOURCE_BEST_OF_MCP, best_of),
    ] {
        match result {
            Ok(batch) => {
                tracing::info!(
                    source = label,
                    count = batch.len(),
                    "pulled discovery source"
                );
                all_sources.push(batch);
            }
            Err(e) => {
                tracing::error!(
                    source = label,
                    error = %e,
                    "discovery source pull failed — continuing without it"
                );
            }
        }
    }
    // Postcondition: at most one batch per source label. All_sources is bounded
    // by the static label list above (4) so callers never see unexpected growth.
    debug_assert!(
        all_sources.len() <= 4,
        "expected at most one batch per source"
    );
    all_sources
}

/// Union the freshly-merged pull with the previously-known index: any server
/// in `existing_by_slug` that this cycle's sources did NOT re-surface is
/// appended unchanged. A degraded/partial pull therefore never shrinks the
/// catalog; successive partial pulls accumulate toward complete. Returns the
/// count of retained-unseen servers.
fn union_with_existing(
    merged: &mut Vec<EnrichedServer>,
    existing_by_slug: &std::collections::HashMap<String, EnrichedServer>,
) -> usize {
    let seen: std::collections::HashSet<String> = merged.iter().map(|s| s.slug()).collect();
    let mut retained_unseen = 0usize;
    for (slug, doc) in existing_by_slug {
        if !seen.contains(slug) {
            merged.push(doc.clone());
            retained_unseen = retained_unseen.saturating_add(1);
        }
    }
    // Postconditions: union can only grow the merge, by exactly the number
    // of retained servers.
    debug_assert!(merged.len() >= seen.len(), "union must not shrink");
    debug_assert_eq!(
        merged.len(),
        seen.len().saturating_add(retained_unseen),
        "union adds exactly the unseen servers"
    );
    retained_unseen
}

/// Patch newly-merged records with any pre-existing Layer 2 / Layer 3 LLM
/// verdicts (loaded once by the caller) so a fresh refresh doesn't wipe out
/// hours of LLM work. Returns `(preserved_layer2, preserved_layer3)`.
fn preserve_layer_classifications(
    existing_by_slug: &std::collections::HashMap<String, EnrichedServer>,
    merged: &mut [EnrichedServer],
) -> (usize, usize) {
    let mut preserved_layer2 = 0usize;
    let mut preserved_layer3 = 0usize;
    for srv in merged.iter_mut() {
        if let Some(existing) = existing_by_slug.get(&srv.slug()) {
            if srv.layer2_classification.is_none() && existing.layer2_classification.is_some() {
                srv.layer2_classification = existing.layer2_classification.clone();
                preserved_layer2 = preserved_layer2.saturating_add(1);
            }
            if srv.layer3_analysis.is_none() && existing.layer3_analysis.is_some() {
                srv.layer3_analysis = existing.layer3_analysis.clone();
                preserved_layer3 = preserved_layer3.saturating_add(1);
            }
        }
    }
    // Postcondition: every preserved counter is bounded by the merged slice length.
    debug_assert!(
        preserved_layer2 <= merged.len(),
        "preserved_layer2 must not exceed merged size"
    );
    debug_assert!(
        preserved_layer3 <= merged.len(),
        "preserved_layer3 must not exceed merged size"
    );
    (preserved_layer2, preserved_layer3)
}

/// Persist every server doc to Firestore. Returns `(written, errors)` —
/// individual write failures are logged and counted but do not abort the
/// caller (the in-memory cache always updates regardless).
async fn persist_servers_to_firestore(
    db: &FirestoreDb,
    merged: &[EnrichedServer],
) -> (usize, usize) {
    let mut written = 0usize;
    let mut write_errors = 0usize;
    for srv in merged {
        let slug = srv.slug();
        match db
            .fluent()
            .update()
            .in_col(MCP_SERVERS_COLLECTION)
            .document_id(&slug)
            .object(srv)
            .execute::<()>()
            .await
        {
            Ok(_) => written = written.saturating_add(1),
            Err(e) => {
                write_errors = write_errors.saturating_add(1);
                tracing::warn!(slug, error = %e, "failed to write mcp_servers doc");
            }
        }
    }
    // Postcondition: written + errors == merged.len() (every input was accounted for).
    debug_assert_eq!(
        written.saturating_add(write_errors),
        merged.len(),
        "every server must be either written or counted as error"
    );
    (written, write_errors)
}

/// Result of a refresh — useful for the admin endpoint that triggers it.
#[derive(Debug, serde::Serialize)]
pub struct RefreshSummary {
    pub total: usize,
    pub earning_count: usize,
    pub firestore_writes: usize,
    pub firestore_write_errors: usize,
    pub elapsed_ms: u64,
}

/// Result of a Layer 2 classification cycle.
#[derive(Debug, serde::Serialize)]
pub struct Layer2Summary {
    /// Total servers we considered (Layer 1 unconfident).
    pub considered: usize,
    /// Of those, how many we actually called Grok on this cycle (capped).
    pub classified: usize,
    /// Of the classified, how many came back as earning candidates.
    pub new_earning_candidates: usize,
    /// Of the classified, how many came back as composable primitives.
    pub new_primitives: usize,
    pub firestore_writes: usize,
    pub firestore_write_errors: usize,
    pub grok_call_errors: usize,
    pub elapsed_ms: u64,
}

/// Run a Layer 2 LLM classification pass over `mcp_servers/` documents where
/// Layer 1 was not confident AND no `layer2_classification` exists yet. Caps
/// at `MAX_SERVERS_PER_CYCLE` to bound budget exposure.
///
/// This is intentionally serial — Grok is the bottleneck (~2s/call), and at
/// 200 calls/cycle that's ~7 minutes. Caller (HTTP handler) should spawn this
/// in the background instead of awaiting it inline; the cap keeps the worst
/// case bounded.
pub async fn run_layer2_pass(state: &Arc<DiscoveryState>) -> Result<Layer2Summary> {
    let started = std::time::Instant::now();

    let llm = state
        .llm
        .as_ref()
        .context("Layer 2 disabled — XAI_API_KEY not set at startup")?;

    // Pull the full index from Firestore. We could query only unconfident
    // ones, but the dataset is small (~2k docs) so a full pull is fine and
    // saves an index requirement.
    let all_servers = load_from_firestore(&state.db).await?;

    let candidates: Vec<&EnrichedServer> = all_servers
        .iter()
        .filter(|s| !s.classification.confident && s.layer2_classification.is_none())
        .take(MAX_SERVERS_PER_CYCLE)
        .collect();

    let considered = candidates.len();
    let counts = classify_layer2_candidates(llm, &state.db, &candidates).await;

    // Invalidate the in-memory cache (and the search index built from it)
    // so the next earning-candidates query / search sees the new Layer 2
    // verdicts.
    {
        let mut cache = state.cache.lock().await;
        *cache = None;
        drop(cache);
        let mut idx = state.search_index.lock().await;
        *idx = None;
    }

    let elapsed_ms = started.elapsed().as_millis();
    tracing::info!(
        considered,
        classified = counts.classified,
        new_earning = counts.new_earning,
        new_primitives = counts.new_primitives,
        writes = counts.writes,
        write_errors = counts.write_errors,
        grok_errors = counts.grok_errors,
        elapsed_ms,
        "run_layer2_pass complete"
    );

    Ok(build_layer2_summary(considered, &counts, elapsed_ms as u64))
}

/// Run the Grok classify call on every candidate, persist results, and
/// accumulate per-pass counters. Serial by design — Grok is the bottleneck.
async fn classify_layer2_candidates(
    llm: &LlmClassifier,
    db: &FirestoreDb,
    candidates: &[&EnrichedServer],
) -> Layer2RunCounts {
    let mut counts = Layer2RunCounts::default();
    for server in candidates {
        match llm.classify_server(server).await {
            Ok(verdict) => {
                counts.classified = counts.classified.saturating_add(1);
                process_layer2_verdict(db, server, verdict, &mut counts).await;
            }
            Err(e) => {
                counts.grok_errors = counts.grok_errors.saturating_add(1);
                tracing::warn!(server = %server.name, error = %e, "layer2 Grok call failed");
            }
        }
    }
    // Postcondition: total touches never exceed the candidate count.
    debug_assert!(
        counts.classified.saturating_add(counts.grok_errors) <= candidates.len(),
        "classified + grok_errors must not exceed candidates"
    );
    counts
}

/// Format the public Layer2Summary wire object from the threaded counters.
fn build_layer2_summary(
    considered: usize,
    counts: &Layer2RunCounts,
    elapsed_ms: u64,
) -> Layer2Summary {
    debug_assert!(
        counts.classified <= considered,
        "classified must not exceed considered"
    );
    Layer2Summary {
        considered,
        classified: counts.classified,
        new_earning_candidates: counts.new_earning,
        new_primitives: counts.new_primitives,
        firestore_writes: counts.writes,
        firestore_write_errors: counts.write_errors,
        grok_call_errors: counts.grok_errors,
        elapsed_ms,
    }
}

/// Per-pass counters threaded through `run_layer2_pass`. Bundled so each
/// helper can update the relevant counter without expanding the function
/// signature for every new field.
#[derive(Default)]
struct Layer2RunCounts {
    classified: usize,
    new_earning: usize,
    new_primitives: usize,
    writes: usize,
    write_errors: usize,
    grok_errors: usize,
}

/// Apply a single Layer 2 verdict: tag the server with the new classification,
/// update the counters for earning/primitive promotions, and persist the
/// updated record to Firestore. Write failures are logged + counted, never
/// propagated.
async fn process_layer2_verdict(
    db: &FirestoreDb,
    server: &EnrichedServer,
    verdict: models::Layer2Classification,
    counts: &mut Layer2RunCounts,
) {
    // Build the updated record with Layer 2 attached.
    let mut updated = server.clone();
    let became_earning = matches!(
        verdict.cash_flow_direction,
        Some(models::CashFlowDirection::EarnsForAgent)
    ) || matches!(
        verdict.value_to_swarm,
        Some(models::ValueToSwarm::AggregateListing)
    );
    let became_primitive = matches!(
        verdict.value_to_swarm,
        Some(models::ValueToSwarm::Dependency)
    );
    if became_earning && verdict.confidence >= 0.6 {
        counts.new_earning = counts.new_earning.saturating_add(1);
    }
    if became_primitive && verdict.confidence >= 0.6 {
        counts.new_primitives = counts.new_primitives.saturating_add(1);
    }
    updated.layer2_classification = Some(verdict);

    let slug = updated.slug();
    debug_assert!(!slug.is_empty(), "every server must have a non-empty slug");
    match db
        .fluent()
        .update()
        .in_col(MCP_SERVERS_COLLECTION)
        .document_id(&slug)
        .object(&updated)
        .execute::<()>()
        .await
    {
        Ok(_) => counts.writes = counts.writes.saturating_add(1),
        Err(e) => {
            counts.write_errors = counts.write_errors.saturating_add(1);
            tracing::warn!(slug, error = %e, "layer2 write failed");
        }
    }
    // Postcondition: every successful classification produces exactly one
    // touch of the write counters (write or write_error).
    debug_assert!(
        counts.writes.saturating_add(counts.write_errors) <= counts.classified,
        "writes + write_errors must not exceed classified"
    );
}

/// Get the current composable-primitives list. Same caching strategy as
/// `get_earning_candidates` — in-memory cache, fall back to Firestore.
pub async fn get_primitives(state: &Arc<DiscoveryState>) -> Vec<EnrichedServer> {
    {
        let cache = state.cache.lock().await;
        if let Some(c) = cache.as_ref() {
            return c
                .servers
                .iter()
                .filter(|s| s.is_primitive())
                .cloned()
                .collect();
        }
    }

    match load_from_firestore(&state.db).await {
        Ok(servers) => {
            let mut cache = state.cache.lock().await;
            *cache = Some(DiscoveryCache {
                servers: servers.clone(),
                last_refreshed_at: chrono::Utc::now(),
            });
            servers.into_iter().filter(|s| s.is_primitive()).collect()
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to load discovery index for primitives");
            Vec::new()
        }
    }
}

/// Get the current earning-candidates list. If the cache is empty, attempts
/// to load from Firestore. Returns an empty list if both fail (caller can
/// trigger a refresh).
pub async fn get_earning_candidates(state: &Arc<DiscoveryState>) -> Vec<EnrichedServer> {
    {
        let cache = state.cache.lock().await;
        if let Some(c) = cache.as_ref() {
            return c
                .servers
                .iter()
                .filter(|s| s.is_earning_candidate())
                .cloned()
                .collect();
        }
    }

    // Cache miss — try Firestore
    match load_from_firestore(&state.db).await {
        Ok(servers) => {
            // Populate the cache for next time
            let mut cache = state.cache.lock().await;
            *cache = Some(DiscoveryCache {
                servers: servers.clone(),
                last_refreshed_at: chrono::Utc::now(),
            });
            servers
                .into_iter()
                .filter(|s| s.is_earning_candidate())
                .collect()
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to load discovery index from Firestore");
            Vec::new()
        }
    }
}

pub async fn load_from_firestore(db: &FirestoreDb) -> Result<Vec<EnrichedServer>> {
    let docs: Vec<EnrichedServer> = db
        .fluent()
        .select()
        .from(MCP_SERVERS_COLLECTION)
        .obj()
        .query()
        .await
        .context("query mcp_servers from Firestore")?;
    Ok(docs)
}

// -- HTTP handlers for the internal discovery endpoints --

use axum::response::IntoResponse;

/// GET /internal/mcp/earning-candidates → Vec<EnrichedServer>
///
/// Returns servers Layer 1 flagged as earning opportunities. Cache-backed,
/// served from in-memory if fresh, falls back to Firestore on cold start.
pub fn earning_candidates_handler(state: Arc<DiscoveryState>) -> axum::routing::MethodRouter {
    axum::routing::get(move || {
        let state = state.clone();
        async move {
            let candidates = get_earning_candidates(&state).await;
            tracing::info!(
                count = candidates.len(),
                "served /internal/mcp/earning-candidates"
            );
            axum::Json(candidates).into_response()
        }
    })
}

/// POST /internal/mcp/refresh → trigger a fresh pull + classify cycle.
/// Returns the RefreshSummary as JSON.
pub fn refresh_handler(state: Arc<DiscoveryState>) -> axum::routing::MethodRouter {
    axum::routing::post(move || {
        let state = state.clone();
        async move {
            match refresh_discovery(&state).await {
                Ok(summary) => axum::Json(summary).into_response(),
                Err(e) => {
                    tracing::error!(error = %e, "discovery refresh failed");
                    (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("{{\"error\": \"{e}\"}}"),
                    )
                        .into_response()
                }
            }
        }
    })
}

/// GET /internal/mcp/primitives → Vec<EnrichedServer>
///
/// Returns servers tagged as composable primitives (`value_to_swarm =
/// dependency`). Tier-2 priority — these are the building blocks other
/// agents can compose into earning loops.
pub fn primitives_handler(state: Arc<DiscoveryState>) -> axum::routing::MethodRouter {
    axum::routing::get(move || {
        let state = state.clone();
        async move {
            let primitives = get_primitives(&state).await;
            tracing::info!(count = primitives.len(), "served /internal/mcp/primitives");
            axum::Json(primitives).into_response()
        }
    })
}

/// POST /internal/mcp/deep-analyze → run a Layer 3 deep-analysis pass over
/// the top earning candidates. Capped at `deep_analysis::MAX_SERVERS_PER_CYCLE`.
/// Returns a `DeepAnalysisSummary`. This is a relatively expensive call (one
/// HTTP round-trip per npm/GitHub lookup × 50 servers); expect ~60-120s.
pub fn deep_analyze_handler(state: Arc<DiscoveryState>) -> axum::routing::MethodRouter {
    axum::routing::post(move || {
        let state = state.clone();
        async move {
            match deep_analysis::run_layer3_pass(&state).await {
                Ok(summary) => axum::Json(summary).into_response(),
                Err(e) => {
                    tracing::error!(error = %e, "layer3 deep-analysis pass failed");
                    (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("{{\"error\": \"{e}\"}}"),
                    )
                        .into_response()
                }
            }
        }
    })
}

/// POST /internal/mcp/llm-classify → kick off a Layer 2 LLM classification
/// pass over the unconfident remainder. Returns 503 if `XAI_API_KEY` was
/// not set at startup. Awaits the full pass and returns a Layer2Summary.
/// Caller should expect this to take ~5-10 minutes for a full cap-bounded
/// cycle, so prefer triggering via background workflow.
pub fn llm_classify_handler(state: Arc<DiscoveryState>) -> axum::routing::MethodRouter {
    axum::routing::post(move || {
        let state = state.clone();
        async move {
            if state.llm.is_none() {
                return (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "{\"error\": \"Layer 2 disabled — XAI_API_KEY not set\"}".to_string(),
                )
                    .into_response();
            }
            match run_layer2_pass(&state).await {
                Ok(summary) => axum::Json(summary).into_response(),
                Err(e) => {
                    tracing::error!(error = %e, "layer2 pass failed");
                    (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("{{\"error\": \"{e}\"}}"),
                    )
                        .into_response()
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::models::Layer1Classification;

    fn srv(name: &str) -> EnrichedServer {
        EnrichedServer {
            name: name.to_string(),
            title: None,
            description: None,
            endpoint: None,
            transport: None,
            npm_package: None,
            github_repo: None,
            sources: vec!["official".to_string()],
            source_count: 1,
            upstream_quality_score: None,
            upstream_visitors_estimate: None,
            classification: Layer1Classification {
                category: None,
                cash_flow_direction: None,
                currencies: vec![],
                value_to_swarm: None,
                confident: false,
                matched_signals: vec![],
            },
            layer2_classification: None,
            layer3_analysis: None,
            first_seen_at: chrono::Utc::now(),
            last_seen_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn union_never_shrinks_catalog_on_partial_pull() {
        // Existing index has 4 servers; the degraded pull re-saw only 1
        // (plus found 1 new). Union must retain the 3 unseen.
        let existing: std::collections::HashMap<String, EnrichedServer> =
            ["a/1", "a/2", "a/3", "a/4"]
                .into_iter()
                .map(|n| {
                    let s = srv(n);
                    (s.slug(), s)
                })
                .collect();
        let mut merged = vec![srv("a/1"), srv("b/new")];
        let retained = union_with_existing(&mut merged, &existing);
        assert_eq!(retained, 3);
        assert_eq!(merged.len(), 5, "2 pulled + 3 retained");
    }

    #[test]
    fn union_is_noop_when_pull_is_complete() {
        let existing: std::collections::HashMap<String, EnrichedServer> = ["a/1", "a/2"]
            .into_iter()
            .map(|n| {
                let s = srv(n);
                (s.slug(), s)
            })
            .collect();
        let mut merged = vec![srv("a/1"), srv("a/2"), srv("a/3")];
        let retained = union_with_existing(&mut merged, &existing);
        assert_eq!(retained, 0);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn union_from_empty_existing_is_noop() {
        let existing = std::collections::HashMap::new();
        let mut merged = vec![srv("a/1")];
        assert_eq!(union_with_existing(&mut merged, &existing), 0);
        assert_eq!(merged.len(), 1);
    }
}
