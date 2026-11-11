//! Bounded per-caller sliding-window HTTP rate limiter (C8).
//!
//! Protects public HTTP endpoints on `mcp-server` from abusive polling,
//! credential-stuffing, and unbounded map-growth OOMs:
//! - `GET` endpoints: 120 requests / 60s per caller key
//! - `POST` / `DELETE` / write endpoints: 60 requests / 60s per caller key
//! - Hard cap of [`MAX_BUCKETS`] (10,000) distinct keys in memory: on collision
//!   at capacity, expired buckets are swept first; if [`MAX_BUCKETS`] active
//!   callers remain, new callers **fail closed** with `429 Too Many Requests`
//!   rather than growing the map unboundedly.

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Hard ceiling on in-memory rate-limit buckets (C8 OOM guard).
pub const MAX_BUCKETS: usize = 10_000;

/// Public HTTP read limit (`GET`): 120 requests per minute.
pub const PUBLIC_GET_PER_MINUTE: usize = 120;

/// Public HTTP write limit (`POST`, `DELETE`, etc.): 60 requests per minute.
pub const PUBLIC_WRITE_PER_MINUTE: usize = 60;

const WINDOW_SECS: u64 = 60;
const INBOX_SESSION_HEADER: &str = "x-inbox-session";
const X_FORWARDED_FOR_HEADER: &str = "x-forwarded-for";

const RATE_LIMIT_CORS: [(&str, &str); 4] = [
    ("Access-Control-Allow-Origin", "*"),
    ("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS"),
    (
        "Access-Control-Allow-Headers",
        "content-type, authorization, x-inbox-session, x-api-key",
    ),
    ("Access-Control-Expose-Headers", "retry-after, x-request-id"),
];

/// Bounded sliding-window rate limiter.
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, VecDeque<Instant>>>,
    max_requests: usize,
    window: Duration,
    max_buckets: usize,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self::with_max_buckets(max_requests, window, MAX_BUCKETS)
    }

    pub fn with_max_buckets(max_requests: usize, window: Duration, max_buckets: usize) -> Self {
        assert!(max_requests > 0, "max_requests must be > 0");
        assert!(!window.is_zero(), "window must be non-zero");
        assert!(max_buckets > 0, "max_buckets must be > 0");
        Self {
            buckets: Mutex::new(HashMap::new()),
            max_requests,
            window,
            max_buckets,
        }
    }

    /// Check and record one request for `key` at `now`.
    ///
    /// Returns `Ok(())` when allowed, or `Err(retry_after_secs)` when the
    /// caller exceeded `max_requests` in `window` OR when all `max_buckets`
    /// slots are occupied by active callers (fail-closed C8 bound).
    pub fn check_at(&self, key: &str, now: Instant) -> Result<(), u64> {
        let mut map = self
            .buckets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if let Some(deque) = map.get_mut(key) {
            prune_deque(deque, now, self.window);
            if deque.len() >= self.max_requests {
                let retry_after = deque
                    .front()
                    .map(|&oldest| retry_after_secs(oldest, now, self.window))
                    .unwrap_or(1);
                return Err(retry_after);
            }
            deque.push_back(now);
            return Ok(());
        }

        if map.len() >= self.max_buckets {
            sweep_expired(&mut map, now, self.window);
            if map.len() >= self.max_buckets {
                return Err(self.window.as_secs().max(1));
            }
        }

        let mut deque = VecDeque::with_capacity(4);
        deque.push_back(now);
        map.insert(key.to_string(), deque);
        Ok(())
    }

    pub fn check(&self, key: &str) -> Result<(), u64> {
        self.check_at(key, Instant::now())
    }

    #[cfg(test)]
    pub fn bucket_count(&self) -> usize {
        self.buckets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

fn prune_deque(deque: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while let Some(&oldest) = deque.front() {
        if now.saturating_duration_since(oldest) >= window {
            deque.pop_front();
        } else {
            break;
        }
    }
}

fn sweep_expired(map: &mut HashMap<String, VecDeque<Instant>>, now: Instant, window: Duration) {
    for deque in map.values_mut() {
        prune_deque(deque, now, window);
    }
    map.retain(|_, deque| !deque.is_empty());
}

fn retry_after_secs(oldest: Instant, now: Instant, window: Duration) -> u64 {
    let elapsed = now.saturating_duration_since(oldest);
    let remaining = window.saturating_sub(elapsed);
    let secs = remaining.as_secs();
    if remaining.subsec_nanos() > 0 {
        secs.saturating_add(1).max(1)
    } else {
        secs.max(1)
    }
}

/// Derive a bounded, secret-safe rate-limit key from request headers / caller context.
///
/// Priority:
/// 1. `Authorization: Bearer <token>` -> `"bearer:<sha256-hex>"` (never stores raw JWT)
/// 2. `X-Inbox-Session: <id>` -> `"sess:<sha256-hex>"`
/// 3. `api_transport::current_caller()` peer IP -> `"ip:<addr>"`
/// 4. Leftmost `X-Forwarded-For` hop -> `"ip:<hop>"` (bounded to 64 chars or hashed)
/// 5. `"ip:unknown"`
pub fn caller_rate_key(headers: &HeaderMap) -> String {
    if let Some(raw) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some((scheme, token)) = raw.trim().split_once(' ') {
            let token = token.trim();
            if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
                return format!("bearer:{}", sha256_hex(token.as_bytes()));
            }
        }
    }

    if let Some(sess) = headers
        .get(INBOX_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return format!("sess:{}", sha256_hex(sess.as_bytes()));
    }

    if let Some(caller) = api_transport::current_caller() {
        return format!("ip:{}", caller.peer.ip());
    }

    if let Some(hop) = headers
        .get(X_FORWARDED_FOR_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| raw.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if hop.len() <= 64 {
            return format!("ip:{hop}");
        }
        return format!("ip:{}", sha256_hex(hop.as_bytes()));
    }

    "ip:unknown".to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Pair of read (`GET`) and write (`POST`/`DELETE`/other) rate limiters for HTTP routes.
#[derive(Clone)]
pub struct HttpRateLimiters {
    pub read: Arc<RateLimiter>,
    pub write: Arc<RateLimiter>,
}

impl Default for HttpRateLimiters {
    fn default() -> Self {
        let window = Duration::from_secs(WINDOW_SECS);
        Self {
            read: Arc::new(RateLimiter::new(PUBLIC_GET_PER_MINUTE, window)),
            write: Arc::new(RateLimiter::new(PUBLIC_WRITE_PER_MINUTE, window)),
        }
    }
}

fn is_exempt_probe_or_preflight(method: &Method, path: &str) -> bool {
    if *method == Method::OPTIONS {
        return true;
    }
    matches!(path, "/health" | "/ready" | "/internal/scaling-metric")
}

pub fn too_many_requests_response(retry_after_secs: u64) -> Response {
    let mut resp = (
        StatusCode::TOO_MANY_REQUESTS,
        RATE_LIMIT_CORS,
        axum::Json(serde_json::json!({
            "error": "rate limit exceeded",
            "reason": "rate_limited",
            "retry_after_secs": retry_after_secs,
        })),
    )
        .into_response();
    if let Ok(val) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        resp.headers_mut()
            .insert(axum::http::header::RETRY_AFTER, val);
    }
    resp
}

/// Axum middleware enforcing C8 bounded rate limits across public HTTP routes.
pub async fn enforce_http_rate_limit(
    State(limiters): State<HttpRateLimiters>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method();
    let path = req.uri().path();
    if is_exempt_probe_or_preflight(method, path) {
        return next.run(req).await;
    }

    let key = caller_rate_key(req.headers());
    let limiter = if *method == Method::GET || *method == Method::HEAD {
        &limiters.read
    } else {
        &limiters.write
    };

    if let Err(retry_after_secs) = limiter.check(&key) {
        tracing::warn!(
            event = "http_rate_limited",
            method = %method,
            path = %path,
            retry_after_secs,
            "HTTP request rejected by bounded rate limiter"
        );
        return too_many_requests_response(retry_after_secs);
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_sliding_window_and_recovers_after_expiry() {
        let limiter = RateLimiter::with_max_buckets(3, Duration::from_secs(60), 16);
        let t0 = Instant::now();

        assert!(limiter.check_at("ip:1.2.3.4", t0).is_ok());
        assert!(limiter
            .check_at(
                "ip:1.2.3.4",
                t0.checked_add(Duration::from_secs(10)).expect("t1")
            )
            .is_ok());
        assert!(limiter
            .check_at(
                "ip:1.2.3.4",
                t0.checked_add(Duration::from_secs(20)).expect("t2")
            )
            .is_ok());

        // 4th call within 60s window rejects with Retry-After > 0.
        let retry_after = limiter
            .check_at(
                "ip:1.2.3.4",
                t0.checked_add(Duration::from_secs(25)).expect("t3"),
            )
            .expect_err("must reject 4th request");
        assert_eq!(retry_after, 35);

        // Once t0 expires at t0 + 60s, one slot frees up.
        assert!(limiter
            .check_at(
                "ip:1.2.3.4",
                t0.checked_add(Duration::from_secs(60)).expect("t4")
            )
            .is_ok());
    }

    #[test]
    fn c8_max_buckets_sweeps_expired_and_fails_closed_when_saturated() {
        let limiter = RateLimiter::with_max_buckets(5, Duration::from_secs(60), 3);
        let t0 = Instant::now();

        assert!(limiter.check_at("k1", t0).is_ok());
        assert!(limiter.check_at("k2", t0).is_ok());
        assert!(limiter.check_at("k3", t0).is_ok());
        assert_eq!(limiter.bucket_count(), 3);

        // At t0 + 5s all 3 buckets are still active -> 4th distinct key MUST fail closed (429)
        // and bucket_count MUST never exceed max_buckets (3).
        let t_active = t0.checked_add(Duration::from_secs(5)).expect("t_active");
        let err = limiter
            .check_at("k4", t_active)
            .expect_err("must fail closed when MAX_BUCKETS saturated");
        assert_eq!(err, 60);
        assert_eq!(limiter.bucket_count(), 3);

        // Existing bucket k1 is still allowed up to its per-key quota.
        assert!(limiter.check_at("k1", t_active).is_ok());

        // At t0 + 61s, k2 and k3 have expired (k1 still has t_active = +5s -> expires at +65s).
        // New key k4 triggers sweep of k2 & k3 and succeeds!
        let t_swept = t0.checked_add(Duration::from_secs(61)).expect("t_swept");
        assert!(limiter.check_at("k4", t_swept).is_ok());
        assert_eq!(limiter.bucket_count(), 2); // k1 + k4
    }

    #[test]
    fn caller_rate_key_hashes_bearer_and_session_and_falls_back_to_xff() {
        let mut h = HeaderMap::new();
        assert_eq!(caller_rate_key(&h), "ip:unknown");

        h.insert(
            X_FORWARDED_FOR_HEADER,
            HeaderValue::from_static("203.0.113.7, 10.0.0.1"),
        );
        assert_eq!(caller_rate_key(&h), "ip:203.0.113.7");

        h.insert(INBOX_SESSION_HEADER, HeaderValue::from_static("sess-abc"));
        let sess_key = caller_rate_key(&h);
        assert!(sess_key.starts_with("sess:"), "{sess_key}");
        assert!(!sess_key.contains("sess-abc"), "must hash session id");

        h.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret.jwt.payload"),
        );
        let bearer_key = caller_rate_key(&h);
        assert!(bearer_key.starts_with("bearer:"), "{bearer_key}");
        assert!(
            !bearer_key.contains("secret.jwt.payload"),
            "must never retain raw JWT in rate-limit key"
        );
    }

    #[tokio::test]
    async fn middleware_exempts_probes_and_preflight_and_returns_429_with_cors() {
        let limiters = HttpRateLimiters {
            read: Arc::new(RateLimiter::with_max_buckets(
                2,
                Duration::from_secs(60),
                16,
            )),
            write: Arc::new(RateLimiter::with_max_buckets(
                1,
                Duration::from_secs(60),
                16,
            )),
        };

        let app = axum::Router::new()
            .route("/health", axum::routing::get(|| async { "ok" }))
            .route(
                "/internal/listings",
                axum::routing::get(|| async { "listings" })
                    .post(|| async { "posted" })
                    .options(|| async { "preflight" }),
            )
            .layer(axum::middleware::from_fn_with_state(
                limiters,
                enforce_http_rate_limit,
            ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let base = format!("http://{addr}");
        let client = reqwest::Client::new();

        // GET allowed twice, 3rd returns 429 + CORS + Retry-After.
        for _ in 0..2u8 {
            let r = client
                .get(format!("{base}/internal/listings"))
                .header("x-forwarded-for", "198.51.100.9")
                .send()
                .await
                .expect("get");
            assert_eq!(r.status(), StatusCode::OK);
        }
        let limited = client
            .get(format!("{base}/internal/listings"))
            .header("x-forwarded-for", "198.51.100.9")
            .send()
            .await
            .expect("get 3");
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            limited
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
        assert!(limited.headers().contains_key(reqwest::header::RETRY_AFTER));

        // Probe (/health) and OPTIONS preflight remain unaffected even for the throttled IP!
        let health = client
            .get(format!("{base}/health"))
            .header("x-forwarded-for", "198.51.100.9")
            .send()
            .await
            .expect("health");
        assert_eq!(health.status(), StatusCode::OK);

        let preflight = client
            .request(
                reqwest::Method::OPTIONS,
                format!("{base}/internal/listings"),
            )
            .header("x-forwarded-for", "198.51.100.9")
            .send()
            .await
            .expect("options");
        assert_eq!(preflight.status(), StatusCode::OK);
    }
}
