//! Firestore-backed `Mcp-Session-Id → wallet` binding table.
//!
//! ## Why
//!
//! The streamable HTTP MCP protocol gives every client a session ID returned
//! in the `Mcp-Session-Id` response header on initialize. The client echoes
//! that ID back on every subsequent tool call. Today the in-memory mapping
//! `session_id → wallet` lives only in `GameSessionManager`'s in-memory
//! HashMap (which `resolve_wallet` falls back to via `get_any_wallet()`),
//! so a Kubernetes rolling restart of the `mcp-server` deployment wipes the
//! mapping for every active session — even though the agent retries with
//! the same session ID and the agent's game state is still in Firestore at
//! `mcp_game_sessions/{wallet}`.
//!
//! This module persists the binding so a new pod can recover. On
//! `game_register_wallet`, the server writes `{ session_id → wallet }` to
//! Firestore. On every later tool call, `resolve_wallet` checks Firestore
//! first (cheap O(1) doc fetch) before falling back to the in-memory map,
//! and on a hit it re-hydrates `GameSessionManager` from the existing
//! per-wallet game session doc.
//!
//! ## What this does NOT do
//!
//! - Does not move rmcp's `WorkerTransport` (the per-session SSE stream
//!   state) across pods. An in-flight SSE stream still dies on pod restart.
//!   That's fine — agents retry with the same session ID, and every tool
//!   call is request-response, not stream-based, so retries are clean.
//! - Does not handle the very-first restart where the binding hasn't been
//!   written yet. Agents must call `game_register_wallet` once after a
//!   restart to seed the binding. One extra tool call, then everything
//!   downstream survives subsequent restarts.

use anyhow::Result;
use firestore::FirestoreDb;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const MCP_HTTP_SESSIONS_COLLECTION: &str = "mcp_http_sessions";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpHttpSessionDoc {
    pub session_id: String,
    pub wallet: String,
    pub created_at: firestore::FirestoreTimestamp,
    pub last_seen_at: firestore::FirestoreTimestamp,
}

pub struct McpSessionBinding {
    db: Arc<FirestoreDb>,
}

impl McpSessionBinding {
    pub fn new(db: Arc<FirestoreDb>) -> Self {
        Self { db }
    }

    /// Persist `session_id → wallet`. Idempotent — re-binding the same
    /// session ID to the same wallet refreshes `last_seen_at`. Re-binding
    /// to a different wallet overwrites the previous mapping (this happens
    /// when an agent calls `game_register_wallet` with a different pubkey
    /// in the same MCP session, which is a legitimate operation).
    pub async fn bind(&self, session_id: &str, wallet: &str) -> Result<()> {
        assert!(!session_id.is_empty(), "session_id must not be empty");
        assert!(!wallet.is_empty(), "wallet must not be empty");

        let now = chrono::Utc::now();
        let doc = McpHttpSessionDoc {
            session_id: session_id.to_string(),
            wallet: wallet.to_string(),
            created_at: firestore::FirestoreTimestamp(now),
            last_seen_at: firestore::FirestoreTimestamp(now),
        };

        if let Err(e) = self
            .db
            .fluent()
            .update()
            .in_col(MCP_HTTP_SESSIONS_COLLECTION)
            .document_id(session_id)
            .object(&doc)
            .execute::<McpHttpSessionDoc>()
            .await
        {
            tracing::warn!(
                session_id = %session_id,
                wallet = %wallet,
                error = %e,
                "failed to persist mcp http session binding (non-fatal — agent can re-register)"
            );
        } else {
            // CONTRACT: the `event` field below is matched by
            // coordination-app/infra/monitoring.tf's
            // `mcp_agent_registrations` log-based metric (filter:
            // jsonPayload.fields.event="register_wallet_bound"). Do not
            // remove or rename this field without updating the metric
            // filter; the alert policy will silently false-positive if
            // the contract drifts.
            tracing::info!(
                event = "register_wallet_bound",
                session_id = %session_id,
                wallet = %wallet,
                "mcp http session bound"
            );
        }
        Ok(())
    }

    /// Look up the wallet bound to `session_id`. Returns `None` if no
    /// binding exists or the lookup fails — in either case the caller falls
    /// back to its own resolution path.
    pub async fn resolve(&self, session_id: &str) -> Option<String> {
        if session_id.is_empty() {
            return None;
        }

        let doc: Option<McpHttpSessionDoc> = self
            .db
            .fluent()
            .select()
            .by_id_in(MCP_HTTP_SESSIONS_COLLECTION)
            .obj()
            .one(session_id)
            .await
            .map_err(|e| {
                tracing::warn!(
                    session_id = %session_id,
                    error = %e,
                    "mcp http session lookup failed"
                );
                e
            })
            .ok()
            .flatten();

        doc.map(|d| d.wallet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Validates the document structure round-trips through serde so a
    /// schema drift would fail compilation rather than at runtime.
    #[test]
    fn doc_serde_roundtrip() {
        let now = firestore::FirestoreTimestamp(chrono::Utc::now());
        let doc = McpHttpSessionDoc {
            session_id: "abc-session".to_string(),
            wallet: "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY".to_string(),
            created_at: now.clone(),
            last_seen_at: now,
        };
        let json = serde_json::to_string(&doc).expect("must serialize");
        let parsed: McpHttpSessionDoc = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(parsed.session_id, "abc-session");
        assert_eq!(
            parsed.wallet,
            "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY"
        );
    }
}
