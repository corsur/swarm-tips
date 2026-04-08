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
    /// Layer 2 classifier — populated only if `XAI_API_KEY` is set in the
    /// environment. When None, the `/internal/mcp/llm-classify` endpoint
    /// returns 503 and `refresh_discovery` skips Layer 2 entirely. Layer 1
    /// keeps working in either case.
    pub llm: Option<LlmClassifier>,
}

impl DiscoveryState {
    pub fn new(db: FirestoreDb, http: reqwest::Client, llm: Option<LlmClassifier>) -> Self {
        Self {
            db,
            http,
            cache: Mutex::new(None),
            llm,
        }
    }
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

    // Merge + classify (synchronous, in-memory). This produces records with
    // layer2_classification = layer3_analysis = None — we then patch them
    // back in from existing Firestore docs below so a fresh refresh doesn't
    // wipe out hours of LLM work.
    let mut merged = merge_and_classify(all_sources);

    // Preserve Layer 2 + Layer 3 across refresh: load the current Firestore
    // docs once, build a slug -> existing-doc map, then for each newly merged
    // record copy any non-None layer2/layer3 fields onto it. The Firestore
    // load is best-effort — if it fails the refresh still completes, we just
    // lose the LLM context (next Layer 2 run regenerates it).
    let existing_by_slug: std::collections::HashMap<String, EnrichedServer> =
        match load_from_firestore(&state.db).await {
            Ok(docs) => docs.into_iter().map(|d| (d.slug(), d)).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "could not load existing index for layer2/3 preservation");
                std::collections::HashMap::new()
            }
        };
    let mut preserved_layer2 = 0usize;
    let mut preserved_layer3 = 0usize;
    for srv in &mut merged {
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

    let total = merged.len();
    let earning_count = merged.iter().filter(|s| s.is_earning_candidate()).count();

    // Persist to Firestore (one doc per server). Best-effort: log + continue
    // on individual write failures, but the cache update happens regardless
    // so the in-memory copy is always fresh.
    let mut written = 0usize;
    let mut write_errors = 0usize;
    for srv in &merged {
        let slug = srv.slug();
        match state
            .db
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

    // Update in-memory cache
    {
        let mut cache = state.cache.lock().await;
        *cache = Some(DiscoveryCache {
            servers: merged,
            last_refreshed_at: chrono::Utc::now(),
        });
    }

    let elapsed_ms = started.elapsed().as_millis();
    tracing::info!(
        total,
        earning_count,
        preserved_layer2,
        preserved_layer3,
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
    let mut classified = 0usize;
    let mut new_earning = 0usize;
    let mut new_primitives = 0usize;
    let mut writes = 0usize;
    let mut write_errors = 0usize;
    let mut grok_errors = 0usize;

    for server in candidates {
        match llm.classify_server(server).await {
            Ok(verdict) => {
                classified = classified.saturating_add(1);

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
                    new_earning = new_earning.saturating_add(1);
                }
                if became_primitive && verdict.confidence >= 0.6 {
                    new_primitives = new_primitives.saturating_add(1);
                }
                updated.layer2_classification = Some(verdict);

                let slug = updated.slug();
                match state
                    .db
                    .fluent()
                    .update()
                    .in_col(MCP_SERVERS_COLLECTION)
                    .document_id(&slug)
                    .object(&updated)
                    .execute::<()>()
                    .await
                {
                    Ok(_) => writes = writes.saturating_add(1),
                    Err(e) => {
                        write_errors = write_errors.saturating_add(1);
                        tracing::warn!(slug, error = %e, "layer2 write failed");
                    }
                }
            }
            Err(e) => {
                grok_errors = grok_errors.saturating_add(1);
                tracing::warn!(server = %server.name, error = %e, "layer2 Grok call failed");
            }
        }
    }

    // Invalidate the in-memory cache so the next earning-candidates query
    // sees the new Layer 2 verdicts.
    {
        let mut cache = state.cache.lock().await;
        *cache = None;
    }

    let elapsed_ms = started.elapsed().as_millis();
    tracing::info!(
        considered,
        classified,
        new_earning,
        new_primitives,
        writes,
        write_errors,
        grok_errors,
        elapsed_ms,
        "run_layer2_pass complete"
    );

    Ok(Layer2Summary {
        considered,
        classified,
        new_earning_candidates: new_earning,
        new_primitives,
        firestore_writes: writes,
        firestore_write_errors: write_errors,
        grok_call_errors: grok_errors,
        elapsed_ms: elapsed_ms as u64,
    })
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
