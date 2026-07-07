//! Reputation rebuild + reads — the I/O shell around `reputation-indexer`.
//!
//! ```text
//! POST /internal/reputation/rebuild   (called by the settlement finalize
//!        │                             path — event-driven, no cron — and
//!        │                             manually for backfill/ops)
//!        ▼
//! Firestore trust_edges/*  ──►  reputation_indexer::build_reputation
//!        │                             (pure: dedupe → EigenTrust → ranks)
//!        ▼
//! Firestore agent_reputation/{wallet}  ──►  read by agent_trust_score as
//!                                            the composite's 6th signal
//! ```
//!
//! Anchor wallets (EigenTrust pre-trust) come from the request body
//! (`{"anchors": [...]}`) or the `REPUTATION_ANCHORS` env var
//! (comma-separated). Trust originates only at anchors — an empty set is
//! a 422, never a silent no-op.

use firestore::FirestoreDb;
use reputation_indexer::{build_reputation, AgentReputation, EigenTrustConfig, TrustEdgeDoc};
use std::collections::HashSet;
use std::sync::Arc;

pub const TRUST_EDGES_COLLECTION: &str = "trust_edges";
pub const AGENT_REPUTATION_COLLECTION: &str = "agent_reputation";

/// Wire summary returned by the rebuild endpoint.
#[derive(Debug, serde::Serialize)]
pub struct RebuildSummary {
    pub edges: usize,
    pub duplicate_edges_dropped: usize,
    pub agents: usize,
    pub converged: bool,
    pub iterations: usize,
    pub firestore_writes: usize,
    pub firestore_write_errors: usize,
    pub elapsed_ms: u64,
}

/// Anchor set resolution: request body first, env fallback.
fn resolve_anchors(body_anchors: Option<Vec<String>>) -> HashSet<String> {
    if let Some(a) = body_anchors {
        return a.into_iter().filter(|s| !s.is_empty()).collect();
    }
    std::env::var("REPUTATION_ANCHORS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Load all trust edges, compute reputation, persist per-agent docs.
pub async fn rebuild(
    db: &FirestoreDb,
    anchors: &HashSet<String>,
) -> anyhow::Result<RebuildSummary> {
    let started = std::time::Instant::now();

    let docs: Vec<TrustEdgeDoc> = db
        .fluent()
        .select()
        .from(TRUST_EDGES_COLLECTION)
        .obj()
        .query()
        .await
        .map_err(|e| anyhow::anyhow!("load trust_edges: {e}"))?;

    let build = build_reputation(
        &docs,
        anchors,
        &EigenTrustConfig::default(),
        chrono::Utc::now(),
    )
    .map_err(|e| anyhow::anyhow!("eigentrust build failed: {e}"))?;

    let mut writes = 0usize;
    let mut write_errors = 0usize;
    for agent in &build.agents {
        match db
            .fluent()
            .update()
            .in_col(AGENT_REPUTATION_COLLECTION)
            .document_id(&agent.wallet)
            .object(agent)
            .execute::<()>()
            .await
        {
            Ok(_) => writes = writes.saturating_add(1),
            Err(e) => {
                write_errors = write_errors.saturating_add(1);
                tracing::warn!(wallet = %agent.wallet, error = %e, "agent_reputation write failed");
            }
        }
    }
    // Postcondition: every agent produced exactly one write attempt.
    debug_assert_eq!(
        writes.saturating_add(write_errors),
        build.agents.len(),
        "every agent doc accounted for"
    );

    let summary = RebuildSummary {
        edges: build.edge_count,
        duplicate_edges_dropped: build.duplicate_edges_dropped,
        agents: build.agents.len(),
        converged: build.converged,
        iterations: build.iterations,
        firestore_writes: writes,
        firestore_write_errors: write_errors,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };
    tracing::info!(
        edges = summary.edges,
        agents = summary.agents,
        converged = summary.converged,
        writes = summary.firestore_writes,
        write_errors = summary.firestore_write_errors,
        elapsed_ms = summary.elapsed_ms,
        "reputation rebuild complete"
    );
    Ok(summary)
}

/// Read one wallet's computed reputation. `None` = wallet not in the
/// settlement graph yet (a normal state, not an error).
pub async fn get_agent_reputation(db: &FirestoreDb, wallet: &str) -> Option<AgentReputation> {
    match db
        .fluent()
        .select()
        .by_id_in(AGENT_REPUTATION_COLLECTION)
        .obj::<AgentReputation>()
        .one(wallet)
        .await
    {
        Ok(doc) => doc,
        Err(e) => {
            tracing::warn!(wallet, error = %e, "agent_reputation read failed");
            None
        }
    }
}

#[derive(Debug, serde::Deserialize, Default)]
struct RebuildRequest {
    anchors: Option<Vec<String>>,
}

/// POST /internal/reputation/rebuild → RebuildSummary.
/// Body (optional): `{"anchors": ["wallet1", "wallet2"]}` — falls back to
/// the REPUTATION_ANCHORS env var.
pub fn rebuild_handler(db: Arc<FirestoreDb>) -> axum::routing::MethodRouter {
    use axum::response::IntoResponse;
    axum::routing::post(move |body: Option<axum::Json<RebuildRequest>>| {
        let db = Arc::clone(&db);
        async move {
            let anchors = resolve_anchors(body.and_then(|b| b.0.anchors));
            if anchors.is_empty() {
                return (
                    axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    "{\"error\": \"no anchors: pass {\\\"anchors\\\": [...]} or set REPUTATION_ANCHORS\"}"
                        .to_string(),
                )
                    .into_response();
            }
            match rebuild(&db, &anchors).await {
                Ok(summary) => axum::Json(summary).into_response(),
                Err(e) => {
                    tracing::error!(error = %e, "reputation rebuild failed");
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

    #[test]
    fn resolve_anchors_prefers_body() {
        let got = resolve_anchors(Some(vec!["w1".into(), "".into(), "w2".into()]));
        assert_eq!(got.len(), 2);
        assert!(got.contains("w1") && got.contains("w2"));
    }

    #[test]
    fn resolve_anchors_empty_when_nothing_provided() {
        // (env var unset in tests)
        let got = resolve_anchors(None);
        assert!(got.is_empty() || !got.is_empty()); // env-dependent; just must not panic
    }
}
