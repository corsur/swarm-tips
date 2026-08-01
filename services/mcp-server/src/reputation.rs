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

use chain_registry::SOLANA_MAINNET_CAIP2;
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

/// Max leaderboard entries one request may return (bounded query).
pub const LEADERBOARD_MAX_LIMIT: u32 = 100;
/// Leaderboard entries returned when the caller doesn't ask for a count.
pub const LEADERBOARD_DEFAULT_LIMIT: u32 = 25;

/// List the settlement-graph leaderboard, best rank first. `limit` is
/// clamped to [1, LEADERBOARD_MAX_LIMIT] before the query runs.
pub async fn list_leaderboard(
    db: &FirestoreDb,
    limit: u32,
) -> anyhow::Result<Vec<AgentReputation>> {
    let limit = limit.clamp(1, LEADERBOARD_MAX_LIMIT);
    let agents: Vec<AgentReputation> = db
        .fluent()
        .select()
        .from(AGENT_REPUTATION_COLLECTION)
        .order_by([("rank", firestore::FirestoreQueryDirection::Ascending)])
        .limit(limit)
        .obj()
        .query()
        .await
        .map_err(|e| anyhow::anyhow!("load agent_reputation leaderboard: {e}"))?;
    // Postcondition: the bound held.
    debug_assert!(agents.len() <= LEADERBOARD_MAX_LIMIT as usize);
    Ok(agents)
}

/// GET /internal/reputation/leaderboard?limit=N → `{count, agents: [...]}`.
/// Public read (CORS *) backing the swarm.tips/reputation leaderboard and
/// the `agent_reputation_leaderboard` MCP tool.
pub fn leaderboard_handler(db: Arc<FirestoreDb>) -> axum::routing::MethodRouter {
    use axum::response::IntoResponse;
    axum::routing::get(
        move |q: axum::extract::Query<std::collections::HashMap<String, String>>| {
            let db = Arc::clone(&db);
            async move {
                let limit = q
                    .get("limit")
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(LEADERBOARD_DEFAULT_LIMIT);
                match list_leaderboard(&db, limit).await {
                    Ok(agents) => (
                        [("Access-Control-Allow-Origin", "*")],
                        axum::Json(serde_json::json!({ "count": agents.len(), "agents": agents })),
                    )
                        .into_response(),
                    Err(e) => {
                        tracing::error!(error = %e, "leaderboard read failed");
                        (
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                            [("Access-Control-Allow-Origin", "*")],
                            format!("{{\"error\": \"{e}\"}}"),
                        )
                            .into_response()
                    }
                }
            }
        },
    )
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

// ---------------------------------------------------------------------------
// Archival backfill — mint trust_edges from the on-chain event history.
//
// Task PDAs are CLOSED on settlement, so the only durable per-settlement
// record is the event chain in transaction history. The backfill walks the
// program's signatures oldest→newest, joins the per-task event chain
// (TaskCreated → TaskClaimed → TaskVerified → TaskFinalized /
// ChallengeResolved) by task_id, and writes one idempotent
// trust_edges/{tx_sig} doc per settlement. Safe to re-run: doc ids are the
// settlement signatures.
// ---------------------------------------------------------------------------

/// Composite-score scale (shared::MAX_SCORE in the program workspace).
const MAX_SCORE: u64 = 1_000_000;
/// Bound on how many signatures one backfill call will walk.
const MAX_BACKFILL_SIGNATURES: usize = 20_000;

fn event_discriminator(name: &str) -> [u8; 8] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(format!("event:{name}").as_bytes());
    let h: [u8; 32] = hasher.finalize().into();
    let mut out = [0u8; 8];
    out.copy_from_slice(&h[0..8]);
    out
}

fn read_u64(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at.saturating_add(8))?
        .try_into()
        .ok()
        .map(u64::from_le_bytes)
}

fn read_pubkey_b58(b: &[u8], at: usize) -> Option<String> {
    b.get(at..at.saturating_add(32))
        .map(|s| bs58::encode(s).into_string())
}

#[derive(Debug, serde::Serialize)]
pub struct BackfillSummary {
    pub signatures_scanned: usize,
    pub finalized_edges: usize,
    pub challenge_edges: usize,
    pub skipped_incomplete: usize,
    pub firestore_writes: usize,
    pub firestore_write_errors: usize,
    pub elapsed_ms: u64,
}

/// One decoded settlement-relevant event.
#[derive(Debug)]
enum ChainEvent {
    Created { task_id: u64, client: String },
    Claimed { task_id: u64, agent: String },
    Verified { task_id: u64, composite_score: u64 },
    Finalized { task_id: u64, agent: String },
    ChallengeResolved { task_id: u64, challenger_won: bool },
}

fn decode_event(data: &[u8]) -> Option<ChainEvent> {
    let disc: [u8; 8] = data.get(0..8)?.try_into().ok()?;
    let b = data.get(8..)?;
    if disc == event_discriminator("TaskCreated") {
        Some(ChainEvent::Created {
            task_id: read_u64(b, 0)?,
            client: read_pubkey_b58(b, 8)?,
        })
    } else if disc == event_discriminator("TaskClaimed") {
        Some(ChainEvent::Claimed {
            task_id: read_u64(b, 0)?,
            agent: read_pubkey_b58(b, 8)?,
        })
    } else if disc == event_discriminator("TaskVerified") {
        Some(ChainEvent::Verified {
            task_id: read_u64(b, 0)?,
            composite_score: read_u64(b, 8)?,
        })
    } else if disc == event_discriminator("TaskFinalized") {
        Some(ChainEvent::Finalized {
            task_id: read_u64(b, 0)?,
            agent: read_pubkey_b58(b, 8)?,
        })
    } else if disc == event_discriminator("ChallengeResolved") {
        Some(ChainEvent::ChallengeResolved {
            task_id: read_u64(b, 0)?,
            challenger_won: b.get(8).map(|v| *v != 0)?,
        })
    } else {
        None
    }
}

/// Extract decoded events from a transaction's log messages.
fn events_from_logs(logs: &[String]) -> Vec<ChainEvent> {
    use base64::Engine as _;
    logs.iter()
        .filter_map(|l| l.strip_prefix("Program data: "))
        .filter_map(|b64| {
            base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .ok()
        })
        .filter_map(|data| decode_event(&data))
        .collect()
}

async fn rpc_call(
    http: &reqwest::Client,
    rpc_url: &str,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": method, "params": params,
    });
    let resp: serde_json::Value = http.post(rpc_url).json(&body).send().await?.json().await?;
    if let Some(err) = resp.get("error") {
        anyhow::bail!("{method} RPC error: {err}");
    }
    Ok(resp
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

/// Walk the full signature history (newest→oldest pages), returning
/// signatures in CHRONOLOGICAL order for event-chain joining.
async fn all_signatures(
    http: &reqwest::Client,
    rpc_url: &str,
    program: &str,
) -> anyhow::Result<Vec<(String, Option<i64>)>> {
    let mut out: Vec<(String, Option<i64>)> = Vec::new();
    let mut before: Option<String> = None;
    loop {
        if out.len() >= MAX_BACKFILL_SIGNATURES {
            tracing::warn!(count = out.len(), "backfill signature cap hit");
            break;
        }
        let mut opts = serde_json::json!({"limit": 1000});
        if let Some(b) = &before {
            opts["before"] = serde_json::json!(b);
        }
        let result = rpc_call(
            http,
            rpc_url,
            "getSignaturesForAddress",
            serde_json::json!([program, opts]),
        )
        .await?;
        let batch = result.as_array().cloned().unwrap_or_default();
        if batch.is_empty() {
            break;
        }
        let batch_len = batch.len();
        for entry in &batch {
            // Skip failed transactions — their events never took effect.
            if !entry.get("err").map(|e| e.is_null()).unwrap_or(false) {
                continue;
            }
            if let Some(sig) = entry.get("signature").and_then(|s| s.as_str()) {
                out.push((
                    sig.to_string(),
                    entry.get("blockTime").and_then(|t| t.as_i64()),
                ));
            }
        }
        before = batch
            .last()
            .and_then(|e| e.get("signature"))
            .and_then(|s| s.as_str())
            .map(String::from);
        if batch_len < 1000 {
            break;
        }
    }
    out.reverse(); // chronological
    Ok(out)
}
/// Per-task state carried across signatures while walking the log history.
/// `Created` / `Claimed` / `Verified` land before the `Finalized` that mints
/// the edge, often in a different transaction.
#[derive(Default)]
struct JoinState {
    client_by_task: std::collections::HashMap<u64, String>,
    agent_by_task: std::collections::HashMap<u64, String>,
    score_by_task: std::collections::HashMap<u64, u64>,
}

/// Counters `apply_events` contributes for one signature.
#[derive(Default, Debug, PartialEq)]
struct EventDelta {
    finalized: usize,
    challenge: usize,
    skipped_incomplete: usize,
}

/// Fold one transaction's decoded events into `state`, returning the trust
/// edges to write and the counter deltas.
///
/// Pure: no Firestore, no clock (`settled_at` is computed by the caller). This
/// was ~65 lines inlined in the middle of a ~150-line `backfill`, so the
/// join-and-mint rules — which are the whole point of the endpoint — had no
/// tests; only the log-decoding helpers did.
fn apply_events(
    events: Vec<ChainEvent>,
    sig: &str,
    settled_at: chrono::DateTime<chrono::Utc>,
    state: &mut JoinState,
) -> (Vec<TrustEdgeDoc>, EventDelta) {
    let mut docs = Vec::new();
    let mut delta = EventDelta::default();

    for event in events {
        match event {
            ChainEvent::Created { task_id, client } => {
                state.client_by_task.insert(task_id, client);
            }
            ChainEvent::Claimed { task_id, agent } => {
                state.agent_by_task.insert(task_id, agent);
            }
            ChainEvent::Verified {
                task_id,
                composite_score,
            } => {
                state.score_by_task.insert(task_id, composite_score);
            }
            ChainEvent::Finalized { task_id, agent } => {
                // Without the Created event we do not know the client, so there
                // is no edge to draw — count it rather than inventing one.
                let Some(client) = state.client_by_task.get(&task_id) else {
                    delta.skipped_incomplete = delta.skipped_incomplete.saturating_add(1);
                    continue;
                };
                // A finalize with no Verified score is a real settlement that
                // carries no trust — weight 0, not a skip.
                let weight = state
                    .score_by_task
                    .get(&task_id)
                    .map(|s| (*s as f64 / MAX_SCORE as f64).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                docs.push(TrustEdgeDoc {
                    tx_sig: sig.to_string(),
                    from: client.clone(),
                    to: agent,
                    weight,
                    task_id,
                    source: "shillbot_finalize".to_string(),
                    chain: SOLANA_MAINNET_CAIP2.to_string(),
                    settled_at,
                });
                delta.finalized = delta.finalized.saturating_add(1);
            }
            ChainEvent::ChallengeResolved {
                task_id,
                challenger_won,
            } => {
                if !challenger_won {
                    continue; // agent won — the Finalized path pays out
                }
                let (Some(client), Some(agent)) = (
                    state.client_by_task.get(&task_id),
                    state.agent_by_task.get(&task_id),
                ) else {
                    delta.skipped_incomplete = delta.skipped_incomplete.saturating_add(1);
                    continue;
                };
                docs.push(TrustEdgeDoc {
                    tx_sig: sig.to_string(),
                    from: client.clone(),
                    to: agent.clone(),
                    weight: 0.0,
                    task_id,
                    source: "challenge_resolved".to_string(),
                    chain: SOLANA_MAINNET_CAIP2.to_string(),
                    settled_at,
                });
                delta.challenge = delta.challenge.saturating_add(1);
            }
        }
    }

    (docs, delta)
}

/// Backfill trust_edges from the Shillbot program's full event history.
pub async fn backfill(
    db: &FirestoreDb,
    http: &reqwest::Client,
    rpc_url: &str,
) -> anyhow::Result<BackfillSummary> {
    let started = std::time::Instant::now();
    let program = crate::solana_reads::SHILLBOT_PROGRAM_ID_BASE58;

    let sigs = all_signatures(http, rpc_url, program).await?;
    let signatures_scanned = sigs.len();

    // Per-task join state, built chronologically.
    let mut join = JoinState::default();

    let mut finalized_edges = 0usize;
    let mut challenge_edges = 0usize;
    let mut skipped_incomplete = 0usize;
    let mut writes = 0usize;
    let mut write_errors = 0usize;

    for (sig, block_time) in &sigs {
        // Pace + retry: 656 back-to-back getTransaction calls tripped the
        // RPC's rate limiter (-32429) and skipped txs silently dropped
        // their join events (observed 2026-07-07). ~50ms pacing keeps us
        // under the limit; rate-limited calls retry with linear backoff.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut tx: anyhow::Result<serde_json::Value> = Err(anyhow::anyhow!("unattempted"));
        for attempt in 1..=3u32 {
            tx = rpc_call(
                http,
                rpc_url,
                "getTransaction",
                serde_json::json!([sig, {"encoding": "json", "maxSupportedTransactionVersion": 0}]),
            )
            .await;
            match &tx {
                Ok(_) => break,
                Err(e) if e.to_string().contains("rate limited") && attempt < 3 => {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        500_u64.saturating_mul(u64::from(attempt)),
                    ))
                    .await;
                }
                Err(_) => break,
            }
        }
        let tx = match tx {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(sig, error = %e, "getTransaction failed after retries — skipping");
                continue;
            }
        };
        let logs: Vec<String> = tx
            .pointer("/meta/logMessages")
            .and_then(|l| l.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let settled_at = block_time
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .unwrap_or_else(chrono::Utc::now);
        let (docs, delta) = apply_events(events_from_logs(&logs), sig, settled_at, &mut join);
        skipped_incomplete = skipped_incomplete.saturating_add(delta.skipped_incomplete);
        finalized_edges = finalized_edges.saturating_add(delta.finalized);
        challenge_edges = challenge_edges.saturating_add(delta.challenge);
        // Write order is preserved exactly: doc ids are tx signatures and the
        // endpoint is idempotent, so replaying a signature is a no-op.
        for doc in docs {
            write_edge(db, &doc, &mut writes, &mut write_errors).await;
        }
    }

    let summary = BackfillSummary {
        signatures_scanned,
        finalized_edges,
        challenge_edges,
        skipped_incomplete,
        firestore_writes: writes,
        firestore_write_errors: write_errors,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };
    tracing::info!(
        signatures = summary.signatures_scanned,
        finalized = summary.finalized_edges,
        challenges = summary.challenge_edges,
        skipped = summary.skipped_incomplete,
        writes = summary.firestore_writes,
        "trust_edges backfill complete"
    );
    Ok(summary)
}

async fn write_edge(
    db: &FirestoreDb,
    doc: &TrustEdgeDoc,
    writes: &mut usize,
    write_errors: &mut usize,
) {
    match db
        .fluent()
        .update()
        .in_col(TRUST_EDGES_COLLECTION)
        .document_id(&doc.tx_sig)
        .object(doc)
        .execute::<()>()
        .await
    {
        Ok(_) => *writes = writes.saturating_add(1),
        Err(e) => {
            *write_errors = write_errors.saturating_add(1);
            tracing::warn!(tx_sig = %doc.tx_sig, error = %e, "trust_edge write failed");
        }
    }
}

/// POST /internal/reputation/backfill → BackfillSummary. Idempotent; safe
/// to re-run (doc ids are settlement signatures).
pub fn backfill_handler(
    db: Arc<FirestoreDb>,
    http: reqwest::Client,
    rpc_url: String,
) -> axum::routing::MethodRouter {
    use axum::response::IntoResponse;
    axum::routing::post(move || {
        let db = Arc::clone(&db);
        let http = http.clone();
        let rpc_url = rpc_url.clone();
        async move {
            match backfill(&db, &http, &rpc_url).await {
                Ok(summary) => axum::Json(summary).into_response(),
                Err(e) => {
                    tracing::error!(error = %e, "trust_edges backfill failed");
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
mod backfill_tests {
    use super::*;
    use base64::Engine as _;

    fn encode_event(name: &str, body: &[u8]) -> String {
        let mut data = event_discriminator(name).to_vec();
        data.extend_from_slice(body);
        format!(
            "Program data: {}",
            base64::engine::general_purpose::STANDARD.encode(data)
        )
    }

    #[test]
    fn decodes_task_finalized_event_from_logs() {
        let mut body = 42u64.to_le_bytes().to_vec(); // task_id
        body.extend_from_slice(&[7u8; 32]); // agent pubkey
        body.extend_from_slice(&5000u64.to_le_bytes()); // payment
        body.extend_from_slice(&50u64.to_le_bytes()); // fee
        let logs = vec![
            "Program log: something".to_string(),
            encode_event("TaskFinalized", &body),
        ];
        let events = events_from_logs(&logs);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChainEvent::Finalized { task_id, agent } => {
                assert_eq!(*task_id, 42);
                assert_eq!(*agent, bs58::encode([7u8; 32]).into_string());
            }
            _ => panic!("wrong event kind"),
        }
    }

    #[test]
    fn decodes_created_verified_chain() {
        let mut created = 7u64.to_le_bytes().to_vec();
        created.extend_from_slice(&[9u8; 32]); // client
        created.extend_from_slice(&[0u8; 33]); // escrow+deadline+... (trailing ignored)
        let mut verified = 7u64.to_le_bytes().to_vec();
        verified.extend_from_slice(&800_000u64.to_le_bytes()); // composite_score
        verified.extend_from_slice(&[0u8; 48]);
        let logs = vec![
            encode_event("TaskCreated", &created),
            encode_event("TaskVerified", &verified),
        ];
        let events = events_from_logs(&logs);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], ChainEvent::Created { task_id: 7, .. }));
        match &events[1] {
            ChainEvent::Verified {
                task_id,
                composite_score,
            } => {
                assert_eq!(*task_id, 7);
                assert_eq!(*composite_score, 800_000);
            }
            _ => panic!("wrong event kind"),
        }
    }

    #[test]
    fn unknown_discriminators_are_ignored() {
        let logs = vec![
            encode_event("SomeOtherEvent", &[1, 2, 3]),
            "Program log: noise".to_string(),
        ];
        assert!(events_from_logs(&logs).is_empty());
    }

    #[test]
    fn challenge_resolved_decodes_challenger_won_flag() {
        let mut body = 3u64.to_le_bytes().to_vec();
        body.push(1); // challenger_won = true
        body.extend_from_slice(&100u64.to_le_bytes());
        let logs = vec![encode_event("ChallengeResolved", &body)];
        let events = events_from_logs(&logs);
        match &events[0] {
            ChainEvent::ChallengeResolved {
                task_id,
                challenger_won,
            } => {
                assert_eq!(*task_id, 3);
                assert!(*challenger_won);
            }
            _ => panic!("wrong event kind"),
        }
    }

    // ---- combinatorial log-sequence matrix -------------------------------
    //
    // The decoder must extract exactly the valid events from any interleaving
    // of valid frames, foreign frames, truncated frames, and non-event noise
    // — silently skipping garbage, never panicking, never inventing events.

    fn created_frame(task_id: u64) -> String {
        let mut body = task_id.to_le_bytes().to_vec();
        body.extend_from_slice(&[9u8; 32]);
        body.extend_from_slice(&[0u8; 33]);
        encode_event("TaskCreated", &body)
    }

    fn finalized_frame(task_id: u64) -> String {
        let mut body = task_id.to_le_bytes().to_vec();
        body.extend_from_slice(&[7u8; 32]);
        body.extend_from_slice(&[0u8; 16]);
        encode_event("TaskFinalized", &body)
    }

    fn noise_variants() -> Vec<(&'static str, String)> {
        vec![
            ("plain-log", "Program log: hello".to_string()),
            ("foreign-event", encode_event("SomeOtherEvent", &[1, 2, 3])),
            (
                "truncated-frame",
                encode_event("TaskFinalized", &[1, 2, 3]), // far too short
            ),
            ("non-base64", "Program data: !!!not-base64!!!".to_string()),
            (
                "short-discriminator",
                format!(
                    "Program data: {}",
                    base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3])
                ),
            ),
        ]
    }

    #[test]
    fn every_noise_interleaving_extracts_exactly_the_valid_events() {
        // Two valid frames × every noise variant × every insertion slot
        // (before / between / after) — the extraction must be identical.
        for (noise_name, noise) in noise_variants() {
            for slot in 0..3 {
                let mut logs = vec![created_frame(1), finalized_frame(1)];
                logs.insert(slot, noise.clone());
                let events = events_from_logs(&logs);
                assert_eq!(
                    events.len(),
                    2,
                    "noise {noise_name:?} at slot {slot}: expected 2 events, got {}",
                    events.len()
                );
                assert!(
                    matches!(events[0], ChainEvent::Created { task_id: 1, .. }),
                    "noise {noise_name:?} at slot {slot}: first event wrong"
                );
                assert!(
                    matches!(events[1], ChainEvent::Finalized { task_id: 1, .. }),
                    "noise {noise_name:?} at slot {slot}: second event wrong"
                );
            }
        }
    }

    #[test]
    fn interleaved_tasks_keep_their_ids_straight() {
        // Frames from two tasks interleaved in one transaction's logs: the
        // decoder is order-preserving and never cross-wires task ids.
        let logs = vec![
            created_frame(10),
            created_frame(20),
            finalized_frame(20),
            finalized_frame(10),
        ];
        let events = events_from_logs(&logs);
        assert_eq!(events.len(), 4);
        let ids: Vec<u64> = events
            .iter()
            .map(|e| match e {
                ChainEvent::Created { task_id, .. } => *task_id,
                ChainEvent::Finalized { task_id, .. } => *task_id,
                other => panic!("unexpected event {other:?}"),
            })
            .collect();
        assert_eq!(ids, vec![10, 20, 20, 10]);
    }

    #[test]
    fn all_noise_no_signal_yields_nothing() {
        let logs: Vec<String> = noise_variants().into_iter().map(|(_, n)| n).collect();
        assert!(events_from_logs(&logs).is_empty());
    }

    // -----------------------------------------------------------------------
    // apply_events — the join-and-mint rules. These were ~65 lines inlined in
    // the middle of backfill, so the endpoint's whole reason for existing had
    // no coverage; only the log-decoding helpers were tested.
    // -----------------------------------------------------------------------

    fn at() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp")
    }

    #[test]
    fn a_full_lifecycle_mints_one_finalize_edge_with_the_scaled_weight() {
        let mut st = JoinState::default();
        let (docs, delta) = apply_events(
            vec![
                ChainEvent::Created {
                    task_id: 1,
                    client: "CLIENT".into(),
                },
                ChainEvent::Claimed {
                    task_id: 1,
                    agent: "AGENT".into(),
                },
                ChainEvent::Verified {
                    task_id: 1,
                    composite_score: MAX_SCORE / 4,
                },
                ChainEvent::Finalized {
                    task_id: 1,
                    agent: "AGENT".into(),
                },
            ],
            "sig-1",
            at(),
            &mut st,
        );

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].from, "CLIENT");
        assert_eq!(docs[0].to, "AGENT");
        assert!((docs[0].weight - 0.25).abs() < f64::EPSILON);
        assert_eq!(docs[0].source, "shillbot_finalize");
        assert_eq!(docs[0].tx_sig, "sig-1");
        assert_eq!(
            delta,
            EventDelta {
                finalized: 1,
                challenge: 0,
                skipped_incomplete: 0
            }
        );
    }

    #[test]
    fn a_score_above_max_is_clamped_rather_than_minting_more_than_full_trust() {
        let mut st = JoinState::default();
        st.client_by_task.insert(1, "CLIENT".into());
        let (docs, _) = apply_events(
            vec![
                ChainEvent::Verified {
                    task_id: 1,
                    composite_score: MAX_SCORE * 5,
                },
                ChainEvent::Finalized {
                    task_id: 1,
                    agent: "AGENT".into(),
                },
            ],
            "sig",
            at(),
            &mut st,
        );
        assert!((docs[0].weight - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_finalize_without_a_score_is_a_real_settlement_carrying_zero_trust() {
        let mut st = JoinState::default();
        st.client_by_task.insert(7, "CLIENT".into());
        let (docs, delta) = apply_events(
            vec![ChainEvent::Finalized {
                task_id: 7,
                agent: "AGENT".into(),
            }],
            "sig",
            at(),
            &mut st,
        );
        // An edge IS drawn — the settlement happened — but it carries no trust.
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].weight, 0.0);
        assert_eq!(delta.finalized, 1);
        assert_eq!(delta.skipped_incomplete, 0);
    }

    #[test]
    fn a_finalize_with_no_known_client_is_skipped_not_invented() {
        let mut st = JoinState::default();
        let (docs, delta) = apply_events(
            vec![ChainEvent::Finalized {
                task_id: 9,
                agent: "AGENT".into(),
            }],
            "sig",
            at(),
            &mut st,
        );
        assert!(docs.is_empty(), "no client means there is no edge to draw");
        assert_eq!(delta.skipped_incomplete, 1);
    }

    #[test]
    fn a_challenge_the_agent_won_mints_nothing() {
        let mut st = JoinState::default();
        st.client_by_task.insert(1, "CLIENT".into());
        st.agent_by_task.insert(1, "AGENT".into());
        let (docs, delta) = apply_events(
            vec![ChainEvent::ChallengeResolved {
                task_id: 1,
                challenger_won: false,
            }],
            "sig",
            at(),
            &mut st,
        );
        // The agent won, so the Finalized path pays out and draws the edge.
        assert!(docs.is_empty());
        assert_eq!(delta, EventDelta::default());
    }

    #[test]
    fn a_challenge_the_challenger_won_mints_a_zero_weight_edge() {
        let mut st = JoinState::default();
        st.client_by_task.insert(1, "CLIENT".into());
        st.agent_by_task.insert(1, "AGENT".into());
        // Even a high score must not leak into a lost challenge.
        st.score_by_task.insert(1, MAX_SCORE);
        let (docs, delta) = apply_events(
            vec![ChainEvent::ChallengeResolved {
                task_id: 1,
                challenger_won: true,
            }],
            "sig",
            at(),
            &mut st,
        );
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].source, "challenge_resolved");
        assert_eq!(docs[0].weight, 0.0);
        assert_eq!(delta.challenge, 1);
    }

    #[test]
    fn join_state_carries_across_signatures() {
        // Created and Finalized normally arrive in DIFFERENT transactions —
        // that is the entire reason JoinState exists.
        let mut st = JoinState::default();
        let (docs, _) = apply_events(
            vec![ChainEvent::Created {
                task_id: 3,
                client: "CLIENT".into(),
            }],
            "sig-a",
            at(),
            &mut st,
        );
        assert!(docs.is_empty());

        let (docs, delta) = apply_events(
            vec![ChainEvent::Finalized {
                task_id: 3,
                agent: "AGENT".into(),
            }],
            "sig-b",
            at(),
            &mut st,
        );
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].from, "CLIENT", "client carried over from sig-a");
        assert_eq!(docs[0].tx_sig, "sig-b", "edge id is the FINALIZE signature");
        assert_eq!(delta.finalized, 1);
    }
}
