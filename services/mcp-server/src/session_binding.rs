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
const INBOX_HTTP_SESSIONS_COLLECTION: &str = "inbox_http_sessions";

#[derive(Clone, Serialize, Deserialize)]
pub struct McpHttpSessionDoc {
    pub session_id: String,
    pub wallet: String,
    pub created_at: firestore::FirestoreTimestamp,
    pub last_seen_at: firestore::FirestoreTimestamp,
    /// The wallet this session has PROVEN ownership of (signed nonce or
    /// on-chain memo tx via game-api's auth endpoints). `None` = merely
    /// registered, never proven.
    #[serde(default)]
    pub verified_wallet: Option<String>,
    /// When the proof was accepted. `#[serde(default)]` so docs written
    /// before this field existed still deserialize.
    #[serde(default)]
    pub verified_at: Option<firestore::FirestoreTimestamp>,
    /// Opaque signed Game/Shillbot API session JWT. Never serialize into tool responses or logs.
    #[serde(default)]
    access_token: Option<String>,
}

pub struct McpSessionBinding {
    db: Option<Arc<FirestoreDb>>,
    collection: &'static str,
    jwt_secret: Option<String>,
    #[cfg(test)]
    mem: std::sync::Mutex<std::collections::HashMap<String, McpHttpSessionDoc>>,
}

impl McpSessionBinding {
    pub fn new(db: Arc<FirestoreDb>) -> Self {
        Self {
            db: Some(db),
            collection: MCP_HTTP_SESSIONS_COLLECTION,
            jwt_secret: None,
            #[cfg(test)]
            mem: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Separate Firestore collection (`inbox_http_sessions`) for REST
    /// `X-Inbox-Session` tokens so REST inbox sessions and `Mcp-Session-Id`
    /// sessions cannot cross-contaminate (`CRIT-07`).
    pub fn new_inbox(db: Arc<FirestoreDb>) -> Self {
        Self {
            db: Some(db),
            collection: INBOX_HTTP_SESSIONS_COLLECTION,
            jwt_secret: None,
            #[cfg(test)]
            mem: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    #[cfg(test)]
    pub fn new_in_memory_for_test() -> Self {
        Self {
            db: None,
            collection: MCP_HTTP_SESSIONS_COLLECTION,
            jwt_secret: None,
            mem: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Attach `jwt-secret` so `resolve_access_token` can mint/refresh 24h HS256
    /// user JWTs for already-verified sessions (e.g. after `piggyback_stake_verification`
    /// or once an earlier JWT nears expiry).
    pub fn with_jwt_secret(mut self, jwt_secret: Option<String>) -> Self {
        self.jwt_secret = jwt_secret.filter(|s| !s.trim().is_empty());
        self
    }

    /// Persist `session_id → wallet`. Idempotent — re-binding the same
    /// session ID to the same wallet refreshes `last_seen_at`. Re-binding
    /// to a different wallet overwrites the previous mapping (this happens
    /// when an agent calls `game_register_wallet` with a different pubkey
    /// in the same MCP session, which is a legitimate operation).
    pub async fn bind(&self, session_id: &str, wallet: &str) -> Result<()> {
        if session_id.trim().is_empty() {
            anyhow::bail!("session_id must not be empty");
        }
        if wallet.trim().is_empty() {
            anyhow::bail!("wallet must not be empty");
        }
        let canonical = crate::jwt::normalize_wallet(wallet);
        let now = firestore::FirestoreTimestamp(chrono::Utc::now());

        let Some(ref db) = self.db else {
            #[cfg(test)]
            {
                let mut map = self.mem.lock().unwrap_or_else(|p| p.into_inner());
                let existing = map.get(session_id).cloned();
                let doc = build_bound_session_doc(existing, session_id, &canonical, now);
                map.insert(session_id.to_owned(), doc);
            }
            return Ok(());
        };

        let existing: Option<McpHttpSessionDoc> = db
            .fluent()
            .select()
            .by_id_in(self.collection)
            .obj()
            .one(session_id)
            .await
            .ok()
            .flatten();

        let doc = build_bound_session_doc(existing, session_id, &canonical, now);

        if let Err(e) = db
            .fluent()
            .update()
            .in_col(self.collection)
            .document_id(session_id)
            .object(&doc)
            .execute::<McpHttpSessionDoc>()
            .await
        {
            tracing::warn!(
                session_id = %session_id,
                wallet = %canonical,
                collection = self.collection,
                error = %e,
                "failed to persist http session binding (non-fatal — agent can re-register)"
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
                wallet = %canonical,
                "mcp http session bound"
            );
        }
        Ok(())
    }

    /// Atomically bind and mark `(session_id, wallet, access_token)` as verified
    /// (used when a stateless client authenticates directly with `Authorization: Bearer <jwt>`).
    pub async fn bind_verified_with_token(
        &self,
        session_id: &str,
        verified: &VerifiedWallet,
        token: &str,
    ) -> Result<()> {
        if session_id.trim().is_empty() || session_id.contains('/') {
            anyhow::bail!("invalid session_id");
        }
        let canonical = crate::jwt::normalize_wallet(verified.as_str());
        let now = firestore::FirestoreTimestamp(chrono::Utc::now());
        let doc = McpHttpSessionDoc {
            session_id: session_id.to_owned(),
            wallet: canonical.clone(),
            created_at: now.clone(),
            last_seen_at: now.clone(),
            verified_wallet: Some(canonical),
            verified_at: Some(now),
            access_token: (!token.is_empty()).then(|| token.to_owned()),
        };
        let Some(ref db) = self.db else {
            #[cfg(test)]
            {
                self.mem
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(session_id.to_owned(), doc);
            }
            return Ok(());
        };
        db.fluent()
            .update()
            .in_col(self.collection)
            .document_id(session_id)
            .object(&doc)
            .execute::<McpHttpSessionDoc>()
            .await?;
        Ok(())
    }

    /// Look up the wallet bound to `session_id`. Returns `None` if no
    /// binding exists or the lookup fails — in either case the caller falls
    /// back to its own resolution path.
    pub async fn resolve(&self, session_id: &str) -> Option<String> {
        if session_id.is_empty() {
            return None;
        }
        let Some(ref db) = self.db else {
            #[cfg(test)]
            {
                return self
                    .mem
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .get(session_id)
                    .map(|d| crate::jwt::normalize_wallet(&d.wallet));
            }
            #[cfg(not(test))]
            {
                return None;
            }
        };

        let doc: Option<McpHttpSessionDoc> = db
            .fluent()
            .select()
            .by_id_in(self.collection)
            .obj()
            .one(session_id)
            .await
            .map_err(|e| {
                tracing::warn!(
                    session_id = %session_id,
                    collection = self.collection,
                    error = %e,
                    "http session lookup failed"
                );
                e
            })
            .ok()
            .flatten();

        doc.map(|d| crate::jwt::normalize_wallet(&d.wallet))
    }

    /// Mark the session's bound wallet as ownership-proven. Read-modify-write:
    /// the verification only lands if the doc still binds `wallet` (a
    /// concurrent re-bind to a different wallet must not inherit the proof).
    /// Errors propagate — a verification the caller believes happened but
    /// didn't persist would silently deny every later inbox call.
    pub async fn mark_verified(&self, session_id: &str, wallet: &str) -> Result<()> {
        self.mark_verified_with_token(session_id, wallet, None)
            .await
    }

    pub async fn mark_verified_with_token(
        &self,
        session_id: &str,
        wallet: &str,
        token: Option<&str>,
    ) -> Result<()> {
        anyhow::ensure!(
            !session_id.is_empty() && !wallet.trim().is_empty(),
            "missing session identity"
        );
        let canonical = crate::jwt::normalize_wallet(wallet);
        let Some(ref db) = self.db else {
            #[cfg(test)]
            {
                let mut map = self.mem.lock().unwrap_or_else(|p| p.into_inner());
                let Some(doc) = map
                    .get_mut(session_id)
                    .filter(|doc| crate::jwt::wallets_match(&doc.wallet, &canonical))
                else {
                    anyhow::bail!("session binding changed during verification");
                };
                let now = firestore::FirestoreTimestamp(chrono::Utc::now());
                doc.wallet = canonical.clone();
                doc.verified_wallet = Some(canonical);
                doc.verified_at = Some(now.clone());
                doc.last_seen_at = now;
                if let Some(token) = token {
                    doc.access_token = Some(token.to_owned());
                }
            }
            return Ok(());
        };
        let collection = self.collection;
        db.run_transaction_with_options(
            |tx_db, tx| {
                let session_id = session_id.to_owned();
                let wallet = canonical.clone();
                let token = token.map(str::to_owned);
                Box::pin(async move {
                    let doc: Option<McpHttpSessionDoc> = tx_db
                        .fluent()
                        .select()
                        .by_id_in(collection)
                        .obj()
                        .one(&session_id)
                        .await
                        .map_err(backoff::Error::Permanent)?;
                    let Some(mut doc) =
                        doc.filter(|doc| crate::jwt::wallets_match(&doc.wallet, &wallet))
                    else {
                        return Err(backoff::Error::Permanent(
                            firestore::errors::FirestoreError::DataConflictError(
                                firestore::errors::FirestoreDataConflictError::new(
                                    firestore::errors::FirestoreErrorPublicGenericDetails::new(
                                        "session_changed".into(),
                                    ),
                                    "session binding changed during verification".into(),
                                ),
                            ),
                        ));
                    };
                    let now = firestore::FirestoreTimestamp(chrono::Utc::now());
                    doc.wallet = wallet.clone();
                    doc.verified_wallet = Some(wallet);
                    doc.verified_at = Some(now.clone());
                    doc.last_seen_at = now;
                    if let Some(token) = token {
                        doc.access_token = Some(token);
                    }
                    tx_db
                        .fluent()
                        .update()
                        .in_col(collection)
                        .document_id(session_id)
                        .object(&doc)
                        .add_to_transaction(tx)
                        .map_err(backoff::Error::Permanent)?;
                    Ok(())
                })
            },
            firestore::FirestoreTransactionOptions::new()
                .with_max_elapsed_time(chrono::Duration::seconds(15)),
        )
        .await?;
        Ok(())
    }

    /// Returns a valid signed JWT tied to this session's current proven wallet.
    ///
    /// When the session is already ownership-proven (`verified_wallet == wallet`)
    /// and its stored `access_token` is absent (e.g. after `piggyback_stake_verification`)
    /// OR expired / expiring within 60s, automatically mints a fresh 24h HS256
    /// user JWT using `self.jwt_secret` and persists it back to the session doc.
    pub async fn resolve_access_token(&self, session_id: &str) -> Result<Option<(String, String)>> {
        anyhow::ensure!(
            !session_id.is_empty() && !session_id.contains('/'),
            "invalid session identifier"
        );
        let doc: Option<McpHttpSessionDoc> = match self.db {
            Some(ref db) => {
                db.fluent()
                    .select()
                    .by_id_in(self.collection)
                    .obj()
                    .one(session_id)
                    .await?
            }
            None => {
                #[cfg(test)]
                {
                    self.mem
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get(session_id)
                        .cloned()
                }
                #[cfg(not(test))]
                {
                    None
                }
            }
        };
        let Some(doc) = doc else {
            return Ok(None);
        };
        let Some((verified, token, refreshed)) =
            resolve_or_refresh_session_jwt(&doc, self.jwt_secret.as_deref())
        else {
            return Ok(None);
        };
        if refreshed {
            if let Err(e) = self
                .mark_verified_with_token(session_id, verified.as_str(), Some(&token))
                .await
            {
                tracing::warn!(
                    session_id = %session_id,
                    wallet = %verified,
                    error = %e,
                    "failed to persist refreshed 24h session JWT (continuing with in-memory token)"
                );
            }
        }
        Ok(Some((verified.as_str().to_owned(), token)))
    }

    /// The wallet this session has PROVEN, or `None` if the session is
    /// unbound, unproven, or the stored proof is for a different wallet
    /// than the current binding (stale proof after a re-bind race).
    pub async fn resolve_verified(&self, session_id: &str) -> Option<VerifiedWallet> {
        if session_id.is_empty() {
            return None;
        }
        let doc: Option<McpHttpSessionDoc> = match self.db {
            Some(ref db) => db
                .fluent()
                .select()
                .by_id_in(self.collection)
                .obj()
                .one(session_id)
                .await
                .map_err(|e| {
                    tracing::warn!(
                        session_id = %session_id,
                        collection = self.collection,
                        error = %e,
                        "http session verified lookup failed"
                    );
                    e
                })
                .ok()
                .flatten(),
            None => {
                #[cfg(test)]
                {
                    self.mem
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get(session_id)
                        .cloned()
                }
                #[cfg(not(test))]
                {
                    None
                }
            }
        };
        let doc = doc?;
        match doc.verified_wallet {
            Some(v) if crate::jwt::wallets_match(&v, &doc.wallet) && !doc.wallet.is_empty() => {
                Some(VerifiedWallet(crate::jwt::normalize_wallet(&doc.wallet)))
            }
            _ => None,
        }
    }
}

/// Cryptographically proven wallet identity.
///
/// The inner string is private to `session_binding` (constructed only after
/// Firestore session-proof verification, cryptographic HS256 user-JWT verification,
/// or via `#[cfg(test)] new_for_test`), so an unverified wallet string can never
/// mint an internal service token (`jwt::mint_svc_token` / `GameApiProxy::internal_bearer`)
/// at compile time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedWallet(String);

impl VerifiedWallet {
    /// Construct a [`VerifiedWallet`] by cryptographically verifying an HS256 user
    /// Bearer JWT (`typ: "user"`, `aud` containing `"mcp"`).
    pub fn from_user_bearer_jwt(token: &str, jwt_secret: &str) -> Result<Self, &'static str> {
        let wallet = crate::jwt::verify_user_jwt(token, jwt_secret, "mcp")?;
        let canonical = if wallet.starts_with("0x") {
            crate::xchain::evm_account_id(&wallet).unwrap_or(wallet)
        } else {
            wallet
        };
        Ok(Self(crate::jwt::normalize_wallet(&canonical)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Extract the CAIP-2 chain ID and chain-native address while preserving
    /// compile-time proof that the extracted address belongs to this verified wallet.
    pub fn to_xchain_native(&self) -> Option<(String, Self)> {
        let (chain, native) = crate::xchain::resolve_xchain_wallet(&self.0)?;
        Some((chain, Self(crate::jwt::normalize_wallet(&native))))
    }

    #[cfg(test)]
    pub fn new_for_test(wallet: impl Into<String>) -> Self {
        Self(crate::jwt::normalize_wallet(&wallet.into()))
    }
}

impl AsRef<str> for VerifiedWallet {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for VerifiedWallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Pure session-token resolution + 24h auto-refresh helper:
/// - Rejects unproven or mismatched session docs (`None`).
/// - Returns `(verified_wallet, existing_token, false)` if `access_token` is present
///   and valid for more than 60 seconds.
/// - Mints a fresh 24h HS256 user JWT `(verified_wallet, fresh_token, true)` when
///   `access_token` is `None` (e.g. after `piggyback_stake_verification`) or expiring
///   within 60s and `jwt_secret` is configured.
pub(crate) fn resolve_or_refresh_session_jwt(
    doc: &McpHttpSessionDoc,
    jwt_secret: Option<&str>,
) -> Option<(VerifiedWallet, String, bool)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    resolve_or_refresh_session_jwt_at(doc, jwt_secret, now)
}

pub(crate) fn resolve_or_refresh_session_jwt_at(
    doc: &McpHttpSessionDoc,
    jwt_secret: Option<&str>,
    now: u64,
) -> Option<(VerifiedWallet, String, bool)> {
    let verified_raw = doc.verified_wallet.as_deref()?;
    if doc.wallet.is_empty() || !crate::jwt::wallets_match(verified_raw, &doc.wallet) {
        return None;
    }
    let verified = VerifiedWallet(crate::jwt::normalize_wallet(&doc.wallet));

    if let Some(existing) = doc.access_token.as_deref().filter(|t| !t.is_empty()) {
        if !crate::jwt::jwt_needs_refresh_at(
            Some(existing),
            now,
            crate::jwt::USER_TOKEN_REFRESH_LEEWAY_SECS,
        ) {
            return Some((verified, existing.to_owned(), false));
        }
    }

    if let Some(secret) = jwt_secret.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(fresh) = crate::jwt::mint_user_jwt_at(&verified, secret, now) {
            return Some((verified, fresh, true));
        }
    } else {
        tracing::warn!(
            session_id = %doc.session_id,
            wallet = %verified,
            "verified session missing or expiring access_token, and jwt-secret is unavailable to refresh it"
        );
        // If a non-JWT opaque test token (e.g. wiremock unit test) or still-unexpired
        // (<60s left) token is stored without jwt_secret configured, preserve fallback usability.
        if let Some(existing) = doc.access_token.as_deref().filter(|t| !t.is_empty()) {
            if !existing.contains('.') || !crate::jwt::jwt_needs_refresh_at(Some(existing), now, 0)
            {
                return Some((verified, existing.to_owned(), false));
            }
        }
    }
    None
}

/// Build the `McpHttpSessionDoc` to persist on `bind(session_id, wallet)`.
///
/// Re-binding the same session to the same wallet (including EIP-55 mixed-case
/// vs lowercase EVM addresses) is idempotent: it refreshes `last_seen_at` and
/// canonicalizes `wallet` / `verified_wallet` while preserving `created_at` and
/// any existing ownership proof. Re-binding to a different wallet resets the
/// proof so the new wallet cannot inherit the previous wallet's verification.
fn build_bound_session_doc(
    existing: Option<McpHttpSessionDoc>,
    session_id: &str,
    wallet: &str,
    now: firestore::FirestoreTimestamp,
) -> McpHttpSessionDoc {
    let canonical = crate::jwt::normalize_wallet(wallet);
    if let Some(prev) = existing.filter(|d| crate::jwt::wallets_match(&d.wallet, &canonical)) {
        McpHttpSessionDoc {
            session_id: session_id.to_string(),
            wallet: canonical.clone(),
            created_at: prev.created_at,
            last_seen_at: now,
            verified_wallet: prev
                .verified_wallet
                .filter(|v| crate::jwt::wallets_match(v, &canonical))
                .map(|_| canonical),
            verified_at: prev.verified_at,
            access_token: prev.access_token,
        }
    } else {
        McpHttpSessionDoc {
            session_id: session_id.to_string(),
            wallet: canonical,
            created_at: now.clone(),
            last_seen_at: now,
            verified_wallet: None,
            verified_at: None,
            access_token: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            access_token: None,
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

    #[test]
    fn rebinding_same_wallet_preserves_verification_and_switching_wallet_clears_it() {
        let t0 = firestore::FirestoreTimestamp(chrono::Utc::now() - chrono::Duration::seconds(60));
        let t1 = firestore::FirestoreTimestamp(chrono::Utc::now());
        let w1 = "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";
        let w2 = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";

        let verified_doc = McpHttpSessionDoc {
            session_id: "sess-1".to_string(),
            wallet: w1.to_string(),
            created_at: t0.clone(),
            last_seen_at: t0.clone(),
            verified_wallet: Some(w1.to_string()),
            verified_at: Some(t0.clone()),
            access_token: Some("tok-123".to_string()),
        };

        let rebound_same =
            build_bound_session_doc(Some(verified_doc.clone()), "sess-1", w1, t1.clone());
        assert_eq!(rebound_same.created_at, t0);
        assert_eq!(rebound_same.last_seen_at, t1);
        assert_eq!(rebound_same.verified_wallet.as_deref(), Some(w1));
        assert_eq!(rebound_same.access_token.as_deref(), Some("tok-123"));

        let rebound_diff = build_bound_session_doc(Some(verified_doc), "sess-1", w2, t1.clone());
        assert_eq!(rebound_diff.wallet, w2);
        assert_eq!(rebound_diff.created_at, t1);
        assert!(rebound_diff.verified_wallet.is_none());
        assert!(rebound_diff.verified_at.is_none());
        assert!(rebound_diff.access_token.is_none());
    }

    #[test]
    fn rebinding_evm_mixed_case_preserves_verification_and_canonicalizes_lowercase() {
        let t0 = firestore::FirestoreTimestamp(chrono::Utc::now() - chrono::Duration::seconds(60));
        let t1 = firestore::FirestoreTimestamp(chrono::Utc::now());
        let mixed = "eip155:8453:0x996213ed4099707059B8B5D7489FFF23DAC9770D";
        let lower = "eip155:8453:0x996213ed4099707059b8b5d7489fff23dac9770d";

        let verified_doc = McpHttpSessionDoc {
            session_id: "sess-evm".to_string(),
            wallet: mixed.to_string(),
            created_at: t0.clone(),
            last_seen_at: t0.clone(),
            verified_wallet: Some(mixed.to_string()),
            verified_at: Some(t0),
            access_token: Some("tok-evm".to_string()),
        };

        let rebound = build_bound_session_doc(Some(verified_doc), "sess-evm", mixed, t1);
        assert_eq!(rebound.wallet, lower);
        assert_eq!(rebound.verified_wallet.as_deref(), Some(lower));
        assert_eq!(rebound.access_token.as_deref(), Some("tok-evm"));
    }

    #[test]
    fn resolve_or_refresh_mints_24h_jwt_after_stake_piggyback_and_on_near_expiry() {
        let ts = firestore::FirestoreTimestamp(chrono::Utc::now());
        let now = 1_795_000_000_u64;
        let secret = "test-jwt-secret-key-32-bytes-min";
        let sol_wallet = "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";

        // 1. Unproven session NEVER mints a JWT even with jwt_secret configured!
        let unproven = McpHttpSessionDoc {
            session_id: "sess-unproven".to_string(),
            wallet: sol_wallet.to_string(),
            created_at: ts.clone(),
            last_seen_at: ts.clone(),
            verified_wallet: None,
            verified_at: None,
            access_token: None,
        };
        assert!(resolve_or_refresh_session_jwt_at(&unproven, Some(secret), now).is_none());

        // 2. Stake-piggybacked verified session (`access_token == None`) mints a fresh 24h JWT!
        let piggybacked = McpHttpSessionDoc {
            session_id: "sess-stake".to_string(),
            wallet: sol_wallet.to_string(),
            created_at: ts.clone(),
            last_seen_at: ts.clone(),
            verified_wallet: Some(sol_wallet.to_string()),
            verified_at: Some(ts.clone()),
            access_token: None,
        };
        let (vw, minted, did_refresh) =
            resolve_or_refresh_session_jwt_at(&piggybacked, Some(secret), now)
                .expect("mints token for stake-verified session");
        assert!(did_refresh);
        assert_eq!(vw.as_str(), sol_wallet);
        assert_eq!(
            crate::jwt::verify_user_jwt_at(&minted, secret, "shillbot", now).unwrap(),
            sol_wallet
        );

        // 3. Fresh stored JWT (> 60s left) is reused without re-minting.
        let active_doc = McpHttpSessionDoc {
            access_token: Some(minted.clone()),
            ..piggybacked.clone()
        };
        let (_, reused, did_refresh_active) =
            resolve_or_refresh_session_jwt_at(&active_doc, Some(secret), now.saturating_add(3600))
                .expect("reuses active token");
        assert!(!did_refresh_active);
        assert_eq!(reused, minted);

        // 4. Token expiring within 60s (e.g. 30s left) is refreshed to a new 24h JWT!
        let near_exp_now = now.saturating_add(86_370);
        let (_, rotated, did_rotate) =
            resolve_or_refresh_session_jwt_at(&active_doc, Some(secret), near_exp_now)
                .expect("rotates expiring token");
        assert!(did_rotate);
        assert_ne!(rotated, minted);
        assert_eq!(
            crate::jwt::verify_user_jwt_at(
                &rotated,
                secret,
                "shillbot",
                near_exp_now.saturating_add(3600)
            )
            .unwrap(),
            sol_wallet
        );
    }

    #[tokio::test]
    async fn async_resolve_access_token_persists_minted_jwt_and_reuses_it() {
        let secret = "test-jwt-secret-key-32-bytes-min";
        let sol_wallet = "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";
        let binding =
            McpSessionBinding::new_in_memory_for_test().with_jwt_secret(Some(secret.to_string()));

        // Unbound / unproven session resolves None.
        binding.bind("sess-p1", sol_wallet).await.expect("bind");
        assert_eq!(
            binding.resolve("sess-p1").await.as_deref(),
            Some(sol_wallet)
        );
        assert!(binding.resolve_verified("sess-p1").await.is_none());
        assert!(binding
            .resolve_access_token("sess-p1")
            .await
            .expect("ok")
            .is_none());

        // Stake-piggybacked verification marks verified without an access_token.
        binding
            .mark_verified("sess-p1", sol_wallet)
            .await
            .expect("mark_verified");
        let verified = binding
            .resolve_verified("sess-p1")
            .await
            .expect("verified wallet");
        assert_eq!(verified.as_str(), sol_wallet);

        // First resolve_access_token mints and persists a 24h JWT; second call reuses it verbatim!
        let (w1, tok1) = binding
            .resolve_access_token("sess-p1")
            .await
            .expect("ok")
            .expect("minted token");
        assert_eq!(w1, sol_wallet);
        assert_eq!(
            crate::jwt::verify_user_jwt(&tok1, secret, "mcp").expect("valid mcp jwt"),
            sol_wallet
        );
        assert_eq!(
            crate::jwt::verify_user_jwt(&tok1, secret, "shillbot").expect("valid shillbot jwt"),
            sol_wallet
        );

        let (w2, tok2) = binding
            .resolve_access_token("sess-p1")
            .await
            .expect("ok")
            .expect("reused token");
        assert_eq!(w2, sol_wallet);
        assert_eq!(tok2, tok1);
    }
}
