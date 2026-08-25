//! Firestore-backed `Mcp-Session-Id → wallet` binding table.
//!
//! ## Why
//!
//! The streamable HTTP MCP protocol gives every client a session ID via the
//! `Mcp-Session-Id` response header on initialize, echoed back on every
//! tool call. Persisting `session_id → wallet` to Firestore (instead of an
//! in-memory HashMap) lets a new pod recover the binding after a
//! Kubernetes rolling restart, and removes any need for an "any wallet"
//! fallback that would leak wallets across sessions sharing a pod.
//!
//! On `register_wallet` the server writes `{ session_id → wallet }`. On
//! every later tool call, `resolve_wallet` checks Firestore first (cheap
//! O(1) doc fetch); on a hit it re-hydrates `GameSessionManager` from the
//! existing per-wallet game session doc.
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
    /// The wallet this session has PROVEN ownership of (signed nonce or
    /// on-chain memo tx via game-api's auth endpoints). `None` = merely
    /// registered, never proven. `bind()` always writes `None`, so
    /// re-binding (same or different wallet) clears verification — the
    /// whole-doc overwrite is the invalidation, by design.
    #[serde(default)]
    pub verified_wallet: Option<String>,
    /// When the proof was accepted. `#[serde(default)]` so docs written
    /// before this field existed still deserialize.
    #[serde(default)]
    pub verified_at: Option<firestore::FirestoreTimestamp>,
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
            // A fresh bind is always unproven: re-binding overwrites the whole
            // doc, so any prior verification for a different wallet dies here.
            verified_wallet: None,
            verified_at: None,
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

    /// Mark the session's bound wallet as ownership-proven. Read-modify-write:
    /// the verification only lands if the doc still binds `wallet` (a
    /// concurrent re-bind to a different wallet must not inherit the proof).
    /// Errors propagate — a verification the caller believes happened but
    /// didn't persist would silently deny every later inbox call.
    pub async fn mark_verified(&self, session_id: &str, wallet: &str) -> Result<()> {
        assert!(!session_id.is_empty(), "session_id must not be empty");
        assert!(!wallet.is_empty(), "wallet must not be empty");

        let doc: Option<McpHttpSessionDoc> = self
            .db
            .fluent()
            .select()
            .by_id_in(MCP_HTTP_SESSIONS_COLLECTION)
            .obj()
            .one(session_id)
            .await?;
        let mut doc = doc.ok_or_else(|| {
            anyhow::anyhow!("no session binding to verify — call register_wallet first")
        })?;
        anyhow::ensure!(
            doc.wallet == wallet,
            "session binding changed wallet mid-verification (bound {}, verifying {wallet})",
            doc.wallet
        );

        let now = chrono::Utc::now();
        doc.verified_wallet = Some(wallet.to_string());
        doc.verified_at = Some(firestore::FirestoreTimestamp(now));
        doc.last_seen_at = firestore::FirestoreTimestamp(now);
        self.db
            .fluent()
            .update()
            .in_col(MCP_HTTP_SESSIONS_COLLECTION)
            .document_id(session_id)
            .object(&doc)
            .execute::<McpHttpSessionDoc>()
            .await?;
        Ok(())
    }

    /// The wallet this session has PROVEN, or `None` if the session is
    /// unbound, unproven, or the stored proof is for a different wallet
    /// than the current binding (stale proof after a re-bind race).
    pub async fn resolve_verified(&self, session_id: &str) -> Option<String> {
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
                    "mcp http session verified lookup failed"
                );
                e
            })
            .ok()
            .flatten();
        let doc = doc?;
        match doc.verified_wallet {
            Some(v) if v == doc.wallet => Some(doc.wallet),
            _ => None,
        }
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
            last_seen_at: now.clone(),
            verified_wallet: Some("CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY".to_string()),
            verified_at: Some(now),
        };
        let json = serde_json::to_string(&doc).expect("must serialize");
        let parsed: McpHttpSessionDoc = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(parsed.session_id, "abc-session");
        assert_eq!(
            parsed.wallet,
            "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY"
        );
        assert_eq!(
            parsed.verified_wallet.as_deref(),
            Some(parsed.wallet.as_str())
        );
        assert!(parsed.verified_at.is_some());
    }

    /// Back-compat: session docs written before the `verified_*` fields
    /// existed must still deserialize (as unproven). A serde failure here
    /// would strand every pre-rollout live session at the first tool call.
    #[test]
    fn doc_without_verified_fields_still_deserializes_as_unproven() {
        let legacy = r#"{
            "session_id": "old-session",
            "wallet": "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY",
            "created_at": "2026-01-01T00:00:00Z",
            "last_seen_at": "2026-01-01T00:00:00Z"
        }"#;
        let parsed: McpHttpSessionDoc = serde_json::from_str(legacy).expect("legacy deserializes");
        assert_eq!(parsed.session_id, "old-session");
        assert!(parsed.verified_wallet.is_none(), "legacy docs are unproven");
        assert!(parsed.verified_at.is_none());
    }
}
