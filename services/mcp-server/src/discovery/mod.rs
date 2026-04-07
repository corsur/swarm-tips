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
pub mod merge;
pub mod models;
pub mod sources;

use crate::discovery::merge::merge_and_classify;
use crate::discovery::models::{EnrichedServer, MCP_SERVERS_COLLECTION};
use crate::discovery::sources::pull_official_registry;
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
}

impl DiscoveryState {
    pub fn new(db: FirestoreDb, http: reqwest::Client) -> Self {
        Self {
            db,
            http,
            cache: Mutex::new(None),
        }
    }
}

/// Refresh the discovery index: pull from all sources, merge + classify,
/// write to Firestore, update cache. Phase 1 only pulls the official registry.
///
/// Returns the count of merged servers and the count of earning candidates.
pub async fn refresh_discovery(state: &Arc<DiscoveryState>) -> Result<RefreshSummary> {
    let started = std::time::Instant::now();

    // Pull from sources. Each source is best-effort — partial failures are OK.
    let mut all_sources: Vec<Vec<crate::discovery::models::RawServer>> = Vec::new();

    match pull_official_registry(&state.http).await {
        Ok(batch) => {
            tracing::info!(
                source = "official",
                count = batch.len(),
                "pulled official MCP registry"
            );
            all_sources.push(batch);
        }
        Err(e) => {
            tracing::error!(
                source = "official",
                error = %e,
                "official registry pull failed — continuing with no data"
            );
        }
    }

    // Merge + classify (synchronous, in-memory)
    let merged = merge_and_classify(all_sources);
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

async fn load_from_firestore(db: &FirestoreDb) -> Result<Vec<EnrichedServer>> {
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
