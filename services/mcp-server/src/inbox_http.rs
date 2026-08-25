//! Browser-facing REST twins for the agent inbox (`/internal/inbox/*`).
//!
//! Same listings-symmetry doctrine as `/internal/listings` vs
//! `list_earning_opportunities`: the browser surface and the MCP tools share
//! ONE storage layer (`inbox.rs`) and ONE wire shape / event set
//! (`inbox::{send_receipt_json, read_page_json, ack_json, log_*}`). Every
//! quota, bound, TTL, and tier limit is enforced in `inbox.rs` — the
//! enforcement chokepoint holds because this module contains no inbox logic,
//! only transport.
//!
//! ```text
//!   POST /internal/inbox/session                 (no session header)
//!     {wallet}                    → game-api /auth/challenge (base58)
//!                                   or /auth/evm/challenge (0x / eip155)
//!                                   → {wallet, nonce}
//!     {wallet, nonce, signature}  → matching /auth/[evm/]verify
//!                                   → mint uuid session id
//!                                   → session_binding.bind + mark_verified
//!                                   → {session_id, wallet, tier}
//!
//!   X-Inbox-Session: {session_id}                (every call below)
//!     GET  /internal/inbox/messages?thread_id&cursor&limit
//!     POST /internal/inbox/ack   {up_to_cursor}
//!     POST /internal/inbox/send  {to_wallet, body, thread_id?, intent?}
//!       │
//!       └─ resolve_verified(session) → CAIP-10 mailbox
//!          → inbox::{get_messages, ack_messages, send_message}
//!            (same ops, same rejections, same agent_message_* events
//!             as the agent_* MCP tools)
//! ```
//!
//! Session ids minted here live in the same `mcp_http_sessions` collection as
//! MCP `Mcp-Session-Id` bindings — `bind()` + `mark_verified()` +
//! `resolve_verified()` are reused verbatim, so invalidation semantics
//! (re-bind clears verification) are identical across surfaces.

use crate::errors::McpServiceError;
use crate::game_proxy::GameApiProxy;
use crate::inbox::{self, Inbox};
use crate::session_binding::McpSessionBinding;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Browser session header. Deliberately NOT `Mcp-Session-Id`: the REST
/// surface must never be mistaken for the MCP transport.
pub const INBOX_SESSION_HEADER: &str = "x-inbox-session";

/// CORS for browser requests from coordination.game / shillbot.org /
/// swarm.tips — same wildcard-origin policy as `/internal/listings`, plus the
/// custom session header.
const INBOX_CORS_HEADERS: [(&str, &str); 4] = [
    ("Access-Control-Allow-Origin", "*"),
    ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
    (
        "Access-Control-Allow-Headers",
        "content-type, x-inbox-session",
    ),
    ("Access-Control-Max-Age", "3600"),
];

/// Everything the REST twins touch. Constructed once in `main.rs` from the
/// SAME session-binding + inbox handles the MCP tools use.
pub struct InboxHttpState {
    pub game_api: GameApiProxy,
    pub session_binding: Arc<McpSessionBinding>,
    pub inbox: Arc<Inbox>,
    pub inbox_seed_wallets: HashSet<String>,
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

fn preflight_response() -> axum::response::Response {
    let mut builder = axum::http::Response::builder();
    for (name, value) in INBOX_CORS_HEADERS {
        builder = builder.header(name, value);
    }
    builder
        .body(axum::body::Body::empty())
        // Static header set — cannot fail; empty 200 is a safe fallback.
        .unwrap_or_default()
}

fn json_ok(value: serde_json::Value) -> axum::response::Response {
    (INBOX_CORS_HEADERS, axum::Json(value)).into_response()
}

/// Error responses carry the same stable `reason` tokens the MCP surface
/// logs, so frontends can branch without string-matching prose.
fn json_error(status: StatusCode, reason: &str, message: &str) -> axum::response::Response {
    (
        status,
        INBOX_CORS_HEADERS,
        axum::Json(serde_json::json!({ "error": message, "reason": reason })),
    )
        .into_response()
}

fn missing_session_response() -> axum::response::Response {
    json_error(
        StatusCode::UNAUTHORIZED,
        "missing_session",
        "missing X-Inbox-Session header — mint one via POST /internal/inbox/session",
    )
}

// ---------------------------------------------------------------------------
// Pure request parsing (reject-at-boundary, unit-tested without I/O)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum SessionRequest {
    Challenge {
        wallet: String,
    },
    Verify {
        wallet: String,
        nonce: String,
        signature: String,
    },
}

/// Reject unknown fields instead of silently ignoring them — same doctrine as
/// `/internal/mcp/search`: a typo'd field name must be visible, not a silent
/// behavior change.
fn known_fields_only(
    obj: &serde_json::Map<String, serde_json::Value>,
    accepted: &[&str],
) -> Result<(), String> {
    if let Some(bad) = obj.keys().find(|k| !accepted.contains(&k.as_str())) {
        return Err(format!(
            "unknown field '{bad}' — accepted: {}",
            accepted.join(", ")
        ));
    }
    Ok(())
}

fn parse_json_object(raw: &str) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("body must be JSON: {e}"))?;
    match value {
        serde_json::Value::Object(obj) => Ok(obj),
        _ => Err("body must be a JSON object".to_string()),
    }
}

/// A required field must be present AND a string; optional string fields may
/// be omitted or null but not any other type (a number where a string belongs
/// is a caller bug, not something to coerce).
fn string_field(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    required: bool,
) -> Result<Option<String>, String> {
    match obj.get(key) {
        None | Some(serde_json::Value::Null) => {
            if required {
                Err(format!("{key} is required"))
            } else {
                Ok(None)
            }
        }
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn parse_session_request(raw: &str) -> Result<SessionRequest, String> {
    let obj = parse_json_object(raw)?;
    known_fields_only(&obj, &["wallet", "nonce", "signature"])?;
    let wallet = string_field(&obj, "wallet", true)?
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .ok_or("wallet is required")?;
    let nonce = string_field(&obj, "nonce", false)?.filter(|s| !s.is_empty());
    let signature = string_field(&obj, "signature", false)?.filter(|s| !s.is_empty());
    match (nonce, signature) {
        (None, None) => Ok(SessionRequest::Challenge { wallet }),
        (Some(nonce), Some(signature)) => Ok(SessionRequest::Verify {
            wallet,
            nonce,
            signature,
        }),
        _ => Err(
            "pass {wallet} alone for a challenge, or {wallet, nonce, signature} to verify"
                .to_string(),
        ),
    }
}

#[derive(Debug)]
struct SendBody {
    to_wallet: String,
    body: String,
    thread_id: Option<String>,
    intent: Option<String>,
}

/// Shape-only validation: recipient/thread/intent/body-size semantics are
/// enforced by `inbox::send_message` so both surfaces reject identically.
fn parse_send_request(raw: &str) -> Result<SendBody, String> {
    let obj = parse_json_object(raw)?;
    known_fields_only(&obj, &["to_wallet", "body", "thread_id", "intent"])?;
    let to_wallet = string_field(&obj, "to_wallet", true)?.ok_or("to_wallet is required")?;
    let body = string_field(&obj, "body", true)?.ok_or("body is required")?;
    Ok(SendBody {
        to_wallet,
        body,
        thread_id: string_field(&obj, "thread_id", false)?,
        intent: string_field(&obj, "intent", false)?,
    })
}

fn parse_ack_request(raw: &str) -> Result<String, String> {
    let obj = parse_json_object(raw)?;
    known_fields_only(&obj, &["up_to_cursor"])?;
    string_field(&obj, "up_to_cursor", true)?.ok_or_else(|| "up_to_cursor is required".to_string())
}

#[derive(Debug)]
struct MessagesQuery {
    thread_id: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}

fn parse_messages_query(q: &HashMap<String, String>) -> Result<MessagesQuery, String> {
    const ACCEPTED: [&str; 3] = ["thread_id", "cursor", "limit"];
    if let Some(bad) = q.keys().find(|k| !ACCEPTED.contains(&k.as_str())) {
        return Err(format!(
            "unknown parameter '{bad}' — accepted: thread_id, cursor, limit"
        ));
    }
    let get = |k: &str| q.get(k).cloned().filter(|s| !s.is_empty());
    let limit = match get("limit") {
        None => None,
        Some(raw) => Some(
            raw.parse::<u32>()
                .map_err(|_| "limit must be an unsigned integer".to_string())?,
        ),
    };
    Ok(MessagesQuery {
        thread_id: get("thread_id"),
        cursor: get("cursor"),
        limit,
    })
}

fn session_id_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(INBOX_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Auth: session mint (twin of agent_verify_wallet) + per-request guard
// ---------------------------------------------------------------------------

/// Route the challenge to game-api's Solana or EVM nonce machine by native
/// address shape — the same fork `agent_verify_wallet` phase 1 takes.
async fn issue_challenge(game_api: &GameApiProxy, native: &str) -> Result<String, McpServiceError> {
    debug_assert!(!native.is_empty(), "native address resolved upstream");
    let resp = if native.starts_with("0x") {
        game_api.auth_evm_challenge(native).await?
    } else {
        game_api.auth_challenge(native).await?
    };
    Ok(resp.nonce)
}

/// Verify a signed nonce via the matching game-api endpoint (ed25519 for
/// Solana, EIP-191 `personal_sign` for EVM). The JWT is discarded —
/// verification is the product, exactly as in `verify_wallet_proof`.
async fn verify_signed_nonce(
    game_api: &GameApiProxy,
    native: &str,
    nonce: &str,
    signature: &str,
) -> Result<(), McpServiceError> {
    debug_assert!(
        !native.is_empty() && !nonce.is_empty(),
        "validated upstream"
    );
    if native.starts_with("0x") {
        game_api.auth_evm_verify(native, nonce, signature).await?;
    } else {
        game_api.auth_verify(native, nonce, signature).await?;
    }
    Ok(())
}

/// Phase 1 response: `{wallet, nonce, next}` with CORS. Split out so the
/// wiremock tests can drive the full HTTP shape without Firestore.
async fn respond_challenge(game_api: &GameApiProxy, native: &str) -> axum::response::Response {
    match issue_challenge(game_api, native).await {
        Ok(nonce) => json_ok(serde_json::json!({
            "wallet": native,
            "nonce": nonce,
            "next": "sign the nonce with your wallet key (base58 ed25519 for Solana, 0x EIP-191 personal_sign for EVM) and POST {wallet, nonce, signature} back to this endpoint",
        })),
        Err(e) => {
            tracing::error!(wallet = %native, error = %e, "inbox session challenge issuance failed");
            json_error(
                StatusCode::BAD_GATEWAY,
                "challenge_failed",
                &format!("challenge issuance failed: {e}"),
            )
        }
    }
}

/// Phase 2: verify the proof, then mint a uuid session and persist it through
/// the SAME `bind` + `mark_verified` pair `register_wallet` /
/// `agent_verify_wallet` use — same docs, same invalidation semantics.
async fn respond_verify(
    state: &InboxHttpState,
    caip10: &str,
    native: &str,
    nonce: &str,
    signature: &str,
) -> axum::response::Response {
    if let Err(e) = verify_signed_nonce(&state.game_api, native, nonce, signature).await {
        tracing::warn!(
            event = "agent_wallet_verify_failed",
            wallet = %native,
            error = %e,
            "wallet ownership proof rejected"
        );
        return json_error(
            StatusCode::UNAUTHORIZED,
            "verify_failed",
            &format!("wallet ownership proof failed: {e}"),
        );
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    // bind() itself never fails hard (a write error is logged and surfaces
    // below when mark_verified finds no doc); mark_verified errors are fatal
    // because an unpersisted proof would 401 every later call.
    if let Err(e) = state.session_binding.bind(&session_id, caip10).await {
        tracing::error!(wallet = %caip10, error = %e, "inbox session bind failed");
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "persist_failed",
            "proof passed but the session could not be persisted — retry",
        );
    }
    if let Err(e) = state
        .session_binding
        .mark_verified(&session_id, caip10)
        .await
    {
        tracing::error!(wallet = %caip10, error = %e, "inbox session mark_verified failed");
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "persist_failed",
            "proof passed but the session could not be persisted — retry",
        );
    }

    // Same tier-resolution path the tools use (session proof + on-chain
    // verification doc + EigenTrust record).
    let tier = state.inbox.resolve_sender_tier(caip10, true).await;
    tracing::info!(
        event = "agent_wallet_verified",
        method = "signed_nonce",
        wallet = %caip10,
        "wallet ownership proven"
    );
    json_ok(serde_json::json!({
        "session_id": session_id,
        "wallet": caip10,
        "tier": tier.as_str(),
    }))
}

async fn handle_session(state: &InboxHttpState, raw: &str) -> axum::response::Response {
    let req = match parse_session_request(raw) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                event = "inbox_session_rejected",
                reason = "invalid_request",
                error = %e,
                "inbox session request rejected"
            );
            return json_error(StatusCode::BAD_REQUEST, "invalid_request", &e);
        }
    };
    let wallet_in = match &req {
        SessionRequest::Challenge { wallet } | SessionRequest::Verify { wallet, .. } => {
            wallet.clone()
        }
    };
    // One normalization for both phases: the CAIP-10 mailbox identity is what
    // gets bound; its native segment is what game-api's nonce machine keys on
    // (challenge and verify MUST use the same string).
    let caip10 = match inbox::mailbox_address(&wallet_in) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                event = "inbox_session_rejected",
                reason = "invalid_wallet",
                wallet = %wallet_in,
                error = %e,
                "inbox session request rejected"
            );
            return json_error(
                StatusCode::BAD_REQUEST,
                "invalid_wallet",
                &format!("invalid wallet: {e}"),
            );
        }
    };
    let native = inbox::caip10_address(&caip10).to_string();
    match req {
        SessionRequest::Challenge { .. } => respond_challenge(&state.game_api, &native).await,
        SessionRequest::Verify {
            nonce, signature, ..
        } => respond_verify(state, &caip10, &native, &nonce, &signature).await,
    }
}

/// Per-request guard — the REST twin of `require_verified_wallet`: resolve
/// the session through the same `resolve_verified` path; unverified or
/// unknown sessions get a structured-logged 401 and NOTHING else (reads
/// included — same privacy invariant as the tools).
async fn require_verified_mailbox(
    state: &InboxHttpState,
    headers: &axum::http::HeaderMap,
) -> Result<String, axum::response::Response> {
    let Some(session_id) = session_id_from_headers(headers) else {
        inbox::log_rejection("missing_session", "", false);
        return Err(missing_session_response());
    };
    match state.session_binding.resolve_verified(&session_id).await {
        Some(wallet) => inbox::mailbox_address(&wallet).map_err(|e| {
            inbox::log_rejection("invalid_wallet", &wallet, false);
            json_error(
                StatusCode::BAD_REQUEST,
                "invalid_wallet",
                &format!("bound wallet is not mailbox-addressable: {e}"),
            )
        }),
        None => {
            // Log with whatever wallet the session is bound to (may be none)
            // — the rejection itself is the funnel signal.
            let bound = state
                .session_binding
                .resolve(&session_id)
                .await
                .unwrap_or_default();
            inbox::log_rejection(
                "unproven_sender",
                &bound,
                state.inbox_seed_wallets.contains(&bound),
            );
            Err(json_error(
                StatusCode::UNAUTHORIZED,
                "unproven_sender",
                "session is unknown or has not proven wallet ownership: mint one via POST /internal/inbox/session",
            ))
        }
    }
}

/// Map inbox op errors exactly like `map_inbox_error`: rejections
/// log-and-400 with the stable reason token, internals log-and-500.
fn inbox_error_response(
    state: &InboxHttpState,
    e: inbox::InboxError,
    wallet: &str,
) -> axum::response::Response {
    match e {
        inbox::InboxError::Rejected(r) => {
            inbox::log_rejection(
                r.reason(),
                wallet,
                state.inbox_seed_wallets.contains(wallet),
            );
            json_error(StatusCode::BAD_REQUEST, r.reason(), &r.message())
        }
        inbox::InboxError::Internal(err) => {
            tracing::error!(wallet, error = %err, "inbox operation failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                &format!("inbox operation failed: {err}"),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Op handlers — delegate DIRECTLY to inbox::{get_messages, ack_messages,
// send_message}; no inbox logic on this side of the seam.
// ---------------------------------------------------------------------------

async fn handle_get_messages(
    state: &InboxHttpState,
    headers: &axum::http::HeaderMap,
    q: &HashMap<String, String>,
) -> axum::response::Response {
    let me = match require_verified_mailbox(state, headers).await {
        Ok(m) => m,
        Err(resp) => return resp,
    };
    let query = match parse_messages_query(q) {
        Ok(v) => v,
        Err(e) => {
            inbox::log_rejection(
                "invalid_request",
                &me,
                state.inbox_seed_wallets.contains(&me),
            );
            return json_error(StatusCode::BAD_REQUEST, "invalid_request", &e);
        }
    };
    match state
        .inbox
        .get_messages(
            &me,
            query.cursor.as_deref(),
            query.limit,
            query.thread_id.as_deref(),
            None,
        )
        .await
    {
        Ok(page) => {
            inbox::log_messages_read(&me, &page);
            json_ok(inbox::read_page_json(&page))
        }
        Err(e) => inbox_error_response(state, e, &me),
    }
}

async fn handle_ack(
    state: &InboxHttpState,
    headers: &axum::http::HeaderMap,
    raw: &str,
) -> axum::response::Response {
    let me = match require_verified_mailbox(state, headers).await {
        Ok(m) => m,
        Err(resp) => return resp,
    };
    let up_to_cursor = match parse_ack_request(raw) {
        Ok(c) => c,
        Err(e) => {
            inbox::log_rejection(
                "invalid_request",
                &me,
                state.inbox_seed_wallets.contains(&me),
            );
            return json_error(StatusCode::BAD_REQUEST, "invalid_request", &e);
        }
    };
    match state.inbox.ack_messages(&me, &up_to_cursor).await {
        Ok(watermark) => {
            inbox::log_messages_acked(&me, &up_to_cursor);
            json_ok(inbox::ack_json(&watermark))
        }
        Err(e) => inbox_error_response(state, e, &me),
    }
}

async fn handle_send(
    state: &InboxHttpState,
    headers: &axum::http::HeaderMap,
    raw: &str,
) -> axum::response::Response {
    let me = match require_verified_mailbox(state, headers).await {
        Ok(m) => m,
        Err(resp) => return resp,
    };
    let body = match parse_send_request(raw) {
        Ok(b) => b,
        Err(e) => {
            inbox::log_rejection(
                "invalid_request",
                &me,
                state.inbox_seed_wallets.contains(&me),
            );
            return json_error(StatusCode::BAD_REQUEST, "invalid_request", &e);
        }
    };
    let seed = state.inbox_seed_wallets.contains(&me);
    let tier = state.inbox.resolve_sender_tier(&me, true).await;
    match state
        .inbox
        .send_message(inbox::SendRequest {
            from: me.clone(),
            to_wallet: body.to_wallet,
            body: body.body,
            thread_id: body.thread_id,
            intent: body.intent,
            tier,
            seed,
        })
        .await
    {
        Ok(receipt) => {
            inbox::log_message_sent(&me, &receipt, tier, seed);
            json_ok(inbox::send_receipt_json(&receipt))
        }
        Err(e) => inbox_error_response(state, e, &me),
    }
}

// ---------------------------------------------------------------------------
// Route builders (wired in main.rs next to the other /internal/* routes)
// ---------------------------------------------------------------------------

pub fn session_handler(state: Arc<InboxHttpState>) -> axum::routing::MethodRouter {
    axum::routing::post(move |body: String| {
        let state = state.clone();
        async move { handle_session(&state, &body).await }
    })
    .options(|| async { preflight_response() })
}

pub fn messages_handler(state: Arc<InboxHttpState>) -> axum::routing::MethodRouter {
    axum::routing::get(
        move |headers: axum::http::HeaderMap, q: axum::extract::Query<HashMap<String, String>>| {
            let state = state.clone();
            async move { handle_get_messages(&state, &headers, &q).await }
        },
    )
    .options(|| async { preflight_response() })
}

pub fn ack_handler(state: Arc<InboxHttpState>) -> axum::routing::MethodRouter {
    axum::routing::post(move |headers: axum::http::HeaderMap, body: String| {
        let state = state.clone();
        async move { handle_ack(&state, &headers, &body).await }
    })
    .options(|| async { preflight_response() })
}

pub fn send_handler(state: Arc<InboxHttpState>) -> axum::routing::MethodRouter {
    axum::routing::post(move |headers: axum::http::HeaderMap, body: String| {
        let state = state.clone();
        async move { handle_send(&state, &headers, &body).await }
    })
    .options(|| async { preflight_response() })
}

// ---------------------------------------------------------------------------
// Tests — pure seams + wiremock for the game-api auth passthrough (house
// style: no live or mocked Firestore).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SOL_B58: &str = "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";
    const EVM_MIXED: &str = "0x996213ed4099707059B8B5D7489FFF23DAC9770D";
    const EVM_LOWER: &str = "0x996213ed4099707059b8b5d7489fff23dac9770d";

    fn proxy(server: &MockServer) -> GameApiProxy {
        GameApiProxy::new(server.uri()).expect("proxy builds against mock uri")
    }

    // -- session request shape ---------------------------------------------

    #[test]
    fn parse_session_request_challenge_and_verify_shapes() {
        let challenge =
            parse_session_request(&format!(r#"{{"wallet":"{SOL_B58}"}}"#)).expect("challenge");
        assert_eq!(
            challenge,
            SessionRequest::Challenge {
                wallet: SOL_B58.to_string()
            }
        );

        let verify = parse_session_request(&format!(
            r#"{{"wallet":"{SOL_B58}","nonce":"n-1","signature":"sigB58"}}"#
        ))
        .expect("verify");
        assert_eq!(
            verify,
            SessionRequest::Verify {
                wallet: SOL_B58.to_string(),
                nonce: "n-1".to_string(),
                signature: "sigB58".to_string(),
            }
        );
    }

    #[test]
    fn parse_session_request_rejects_unknown_field_naming_accepted_set() {
        let err = parse_session_request(&format!(r#"{{"wallet":"{SOL_B58}","pubkey":"x"}}"#))
            .expect_err("unknown field rejected");
        assert!(err.contains("unknown field 'pubkey'"), "{err}");
        assert!(err.contains("wallet, nonce, signature"), "{err}");
    }

    #[test]
    fn parse_session_request_requires_wallet_and_rejects_non_object() {
        assert!(parse_session_request("{}").is_err());
        assert!(parse_session_request(r#"{"wallet":""}"#).is_err());
        assert!(
            parse_session_request(r#"{"wallet":42}"#).is_err(),
            "non-string wallet"
        );
        assert!(parse_session_request("[]").is_err());
        assert!(parse_session_request("not json").is_err());
    }

    #[test]
    fn parse_session_request_rejects_partial_proof() {
        // nonce without signature (and vice versa) is neither phase — reject
        // rather than guess.
        let err = parse_session_request(&format!(r#"{{"wallet":"{SOL_B58}","nonce":"n"}}"#))
            .expect_err("partial proof");
        assert!(err.contains("challenge"), "{err}");
        assert!(
            parse_session_request(&format!(r#"{{"wallet":"{SOL_B58}","signature":"s"}}"#)).is_err()
        );
    }

    // -- send / ack request shape ------------------------------------------

    #[test]
    fn parse_send_request_minimal_and_full() {
        let min =
            parse_send_request(&format!(r#"{{"to_wallet":"{SOL_B58}","body":"hi"}}"#)).expect("ok");
        assert_eq!(min.to_wallet, SOL_B58);
        assert_eq!(min.body, "hi");
        assert!(min.thread_id.is_none() && min.intent.is_none());

        let full = parse_send_request(&format!(
            r#"{{"to_wallet":"{SOL_B58}","body":"hi","thread_id":"task:1","intent":"task_offer"}}"#
        ))
        .expect("ok");
        assert_eq!(full.thread_id.as_deref(), Some("task:1"));
        assert_eq!(full.intent.as_deref(), Some("task_offer"));
    }

    #[test]
    fn parse_send_request_rejects_unknown_field_and_missing_required() {
        let err = parse_send_request(&format!(
            r#"{{"to_wallet":"{SOL_B58}","body":"hi","subject":"x"}}"#
        ))
        .expect_err("unknown field");
        assert!(err.contains("unknown field 'subject'"), "{err}");
        assert!(err.contains("to_wallet, body, thread_id, intent"), "{err}");

        assert!(
            parse_send_request(r#"{"body":"hi"}"#).is_err(),
            "no to_wallet"
        );
        assert!(
            parse_send_request(&format!(r#"{{"to_wallet":"{SOL_B58}"}}"#)).is_err(),
            "no body"
        );
        // Semantic validation (intent enum, body bytes, recipient) is the
        // storage layer's job — an empty body parses here and is rejected by
        // send_message with the SAME empty_body reason the MCP tool logs.
        assert!(parse_send_request(&format!(r#"{{"to_wallet":"{SOL_B58}","body":""}}"#)).is_ok());
    }

    #[test]
    fn parse_ack_request_shape() {
        assert_eq!(
            parse_ack_request(r#"{"up_to_cursor":"c1"}"#).expect("ok"),
            "c1"
        );
        assert!(parse_ack_request("{}").is_err());
        let err =
            parse_ack_request(r#"{"up_to_cursor":"c1","cursor":"c2"}"#).expect_err("unknown field");
        assert!(err.contains("unknown field 'cursor'"), "{err}");
    }

    // -- messages query shape ----------------------------------------------

    #[test]
    fn parse_messages_query_accepts_known_params() {
        let mut q = HashMap::new();
        q.insert("thread_id".to_string(), "task:1".to_string());
        q.insert("cursor".to_string(), "c1".to_string());
        q.insert("limit".to_string(), "25".to_string());
        let parsed = parse_messages_query(&q).expect("ok");
        assert_eq!(parsed.thread_id.as_deref(), Some("task:1"));
        assert_eq!(parsed.cursor.as_deref(), Some("c1"));
        assert_eq!(parsed.limit, Some(25));

        // Empty values are treated as absent (browser form serialization
        // habitually sends empty strings).
        let mut q = HashMap::new();
        q.insert("thread_id".to_string(), String::new());
        let parsed = parse_messages_query(&q).expect("ok");
        assert!(parsed.thread_id.is_none() && parsed.limit.is_none());
    }

    #[test]
    fn parse_messages_query_rejects_unknown_param_and_bad_limit() {
        let mut q = HashMap::new();
        q.insert("min_trust".to_string(), "0.5".to_string());
        let err = parse_messages_query(&q).expect_err("unknown param");
        assert!(err.contains("unknown parameter 'min_trust'"), "{err}");
        assert!(err.contains("thread_id, cursor, limit"), "{err}");

        let mut q = HashMap::new();
        q.insert("limit".to_string(), "lots".to_string());
        assert!(parse_messages_query(&q).is_err(), "non-numeric limit");
        let mut q = HashMap::new();
        q.insert("limit".to_string(), "-1".to_string());
        assert!(parse_messages_query(&q).is_err(), "negative limit");
    }

    // -- session header guard ----------------------------------------------

    #[test]
    fn session_id_from_headers_requires_nonempty_header() {
        let mut headers = axum::http::HeaderMap::new();
        assert!(session_id_from_headers(&headers).is_none(), "missing");
        headers.insert(
            INBOX_SESSION_HEADER,
            axum::http::HeaderValue::from_static("  "),
        );
        assert!(session_id_from_headers(&headers).is_none(), "blank");
        headers.insert(
            INBOX_SESSION_HEADER,
            axum::http::HeaderValue::from_static("abc-123"),
        );
        assert_eq!(
            session_id_from_headers(&headers).as_deref(),
            Some("abc-123")
        );
    }

    #[test]
    fn missing_session_is_401_with_cors() {
        let resp = missing_session_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
    }

    #[test]
    fn preflight_allows_the_session_header() {
        let resp = preflight_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let allow_headers = resp
            .headers()
            .get("access-control-allow-headers")
            .and_then(|v| v.to_str().ok())
            .expect("allow-headers present");
        assert!(allow_headers.contains("x-inbox-session"), "{allow_headers}");
        assert!(allow_headers.contains("content-type"), "{allow_headers}");
    }

    // -- game-api auth passthrough (wiremock) ------------------------------

    #[tokio::test]
    async fn challenge_routes_base58_to_solana_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/challenge"))
            .and(body_partial_json(json!({ "wallet": SOL_B58 })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "nonce": "n-sol" })))
            .expect(1)
            .mount(&server)
            .await;

        let nonce = issue_challenge(&proxy(&server), SOL_B58).await.expect("ok");
        assert_eq!(nonce, "n-sol");
    }

    #[tokio::test]
    async fn challenge_routes_evm_wallet_to_evm_endpoint_normalized() {
        // Mixed-case (EIP-55) input must reach game-api as the SAME
        // lowercased native string the verify phase will use — the nonce
        // store is keyed by wallet string.
        let caip10 = inbox::mailbox_address(EVM_MIXED).expect("valid");
        let native = inbox::caip10_address(&caip10);
        assert_eq!(native, EVM_LOWER);

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/evm/challenge"))
            .and(body_partial_json(json!({ "wallet": EVM_LOWER })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "nonce": "n-evm" })))
            .expect(1)
            .mount(&server)
            .await;

        let nonce = issue_challenge(&proxy(&server), native).await.expect("ok");
        assert_eq!(nonce, "n-evm");
    }

    #[tokio::test]
    async fn challenge_phase_response_carries_nonce_and_cors() {
        // Handler-level: the full phase-1 HTTP response (status, CORS, body).
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/challenge"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "nonce": "n-1" })))
            .expect(1)
            .mount(&server)
            .await;

        let resp = respond_challenge(&proxy(&server), SOL_B58).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .expect("body");
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(body["nonce"], "n-1");
        assert_eq!(body["wallet"], SOL_B58);
    }

    #[tokio::test]
    async fn verify_routes_ed25519_to_solana_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/verify"))
            .and(body_partial_json(json!({
                "wallet": SOL_B58,
                "nonce": "n-1",
                "signature": "sigB58",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "jwt-1" })))
            .expect(1)
            .mount(&server)
            .await;

        verify_signed_nonce(&proxy(&server), SOL_B58, "n-1", "sigB58")
            .await
            .expect("ok");
    }

    #[tokio::test]
    async fn verify_routes_eip191_to_evm_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/evm/verify"))
            .and(body_partial_json(json!({
                "wallet": EVM_LOWER,
                "nonce": "n-evm",
                "signature": "0xsig",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "jwt-evm" })))
            .expect(1)
            .mount(&server)
            .await;

        verify_signed_nonce(&proxy(&server), EVM_LOWER, "n-evm", "0xsig")
            .await
            .expect("ok");
    }

    #[tokio::test]
    async fn verify_failure_propagates_as_error() {
        // A wrong signature must reject loudly — this is the security path.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/verify"))
            .respond_with(
                ResponseTemplate::new(401).set_body_string(r#"{"error":"invalid_signature"}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = verify_signed_nonce(&proxy(&server), SOL_B58, "n", "bad")
            .await
            .expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains("401"), "carries status: {msg}");
    }
}
