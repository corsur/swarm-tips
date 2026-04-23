pub mod filters;
pub mod models;
pub mod sources;
pub mod spending;

use chrono::{DateTime, Utc};
use firestore::FirestoreDb;
use models::{
    AgentJob, HealthCheck, IngestionConfig, ListingDoc, ListingEventDoc, RawListing,
    SourceHealthDoc, INGESTION_CONFIG, LISTINGS, LISTING_EVENTS, SOURCE_HEALTH,
};
use sources::FetchResult;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Cached listings with a TTL to avoid hammering external APIs on rapid successive calls.
pub struct ListingsCache {
    listings: Vec<AgentJob>,
    fetched_at: chrono::DateTime<Utc>,
}

const CACHE_TTL_SECS: i64 = 300; // 5 minutes

/// Number of consecutive failures before we back a source off.
const BACKOFF_FAILURE_THRESHOLD: u32 = 3;

/// How long to park a source after it crosses [`BACKOFF_FAILURE_THRESHOLD`].
/// Long enough to meaningfully slow request volume against a 1027-blocked
/// upstream (moltlaunch behind Cloudflare); short enough to recover within a
/// day if the block lifts.
const BACKOFF_WINDOW_SECS: i64 = 60 * 60 * 6; // 6 hours

/// Per-source rate-limit bookkeeping. Lives in memory on each pod, so a pod
/// restart resets it — that's acceptable because the outer 5-minute listing
/// cache already dampens post-restart traffic.
#[derive(Default)]
struct SourceBackoff {
    /// Consecutive failure count. Reset to 0 on any success.
    consecutive_failures: HashMap<String, u32>,
    /// Timestamp after which the source is eligible to be fetched again.
    /// If `now < skip_until`, the source is skipped this cycle.
    skip_until: HashMap<String, DateTime<Utc>>,
}

/// State for the listings subsystem, added to SharedState.
pub struct ListingsState {
    pub db: FirestoreDb,
    pub http_client: reqwest::Client,
    cache: Mutex<Option<ListingsCache>>,
    backoff: Mutex<SourceBackoff>,
}

impl ListingsState {
    pub fn new(db: FirestoreDb, http_client: reqwest::Client) -> Self {
        Self {
            db,
            http_client,
            cache: Mutex::new(None),
            backoff: Mutex::new(SourceBackoff::default()),
        }
    }
}

/// A source fetch "succeeded" iff the HTTP transport completed with a 2xx and
/// no decode/connect error. An empty listing array from a 200 response is
/// still a success — the upstream simply had nothing to offer. Only actual
/// HTTP errors (429, 5xx, timeouts, DNS failures) count as failures for the
/// backoff + disappearance logic.
fn is_fetch_success(result: &FetchResult) -> bool {
    let sc = result.health.status_code;
    result.health.error.is_none() && (200..=299).contains(&sc)
}

/// Apply one cycle's fetch result to the in-memory backoff state.
/// Extracted from `get_listings` so the state transitions can be tested.
fn apply_backoff_update(bk: &mut SourceBackoff, result: &FetchResult, now: DateTime<Utc>) {
    if result.health.error.as_deref() == Some("skipped_backoff") {
        // Skipped sources don't update state — they just pass through.
        return;
    }
    if is_fetch_success(result) {
        bk.consecutive_failures.remove(&result.source);
        bk.skip_until.remove(&result.source);
        return;
    }
    let new_count = bk
        .consecutive_failures
        .get(&result.source)
        .copied()
        .unwrap_or(0)
        .saturating_add(1);
    bk.consecutive_failures
        .insert(result.source.clone(), new_count);
    if new_count >= BACKOFF_FAILURE_THRESHOLD {
        // checked_add_signed handles the unlikely case of timestamp overflow
        // (saturates to None); falling back to `now` is fine — it just means
        // the source is eligible again immediately and we re-park next cycle.
        let until = now
            .checked_add_signed(chrono::Duration::seconds(BACKOFF_WINDOW_SECS))
            .unwrap_or(now);
        bk.skip_until.insert(result.source.clone(), until);
    }
}

/// Fetch listings, update Firestore, return filtered results.
/// This is called by the GET /internal/listings endpoint.
pub async fn get_listings(state: &Arc<ListingsState>) -> Result<Vec<AgentJob>, anyhow::Error> {
    // Check cache
    {
        let cache = state.cache.lock().await;
        if let Some(ref cached) = *cache {
            let age = Utc::now()
                .signed_duration_since(cached.fetched_at)
                .num_seconds();
            if age < CACHE_TTL_SECS {
                tracing::debug!(cache_age_secs = age, "serving listings from cache");
                return Ok(cached.listings.clone());
            }
        }
    }

    // Fetch from all sources in parallel — but skip any source currently in
    // backoff (see SourceBackoff). Sources that have been failing repeatedly
    // are parked for BACKOFF_WINDOW_SECS so we're not hammering Cloudflare-
    // blocked endpoints every 5 min. Skipped sources get a synthetic
    // `status_code = 0, error = "skipped_backoff"` health entry so we don't
    // mistake them for a successful empty response.
    //
    // ClawTasks was removed 2026-04-08 (centralized API was returning HTTP
    // 500 on every endpoint and the strategic shift to
    // unified-list-tools-with-redirect retired centralized full-CRUD
    // proxies). See docs/analysis/2026-04-08-unified-list-tools-strategic-shift.md.
    let client = &state.http_client;
    let skipped = {
        let bk = state.backoff.lock().await;
        let now = Utc::now();
        let mut set = HashSet::new();
        for (source, until) in bk.skip_until.iter() {
            if now < *until {
                set.insert(source.clone());
            }
        }
        set
    };

    let (botbounty, bountycaster, moltlaunch, shillbot, defillama) = tokio::join!(
        fetch_if_not_skipped(&skipped, "botbounty", sources::fetch_botbounty(client)),
        fetch_if_not_skipped(
            &skipped,
            "bountycaster",
            sources::fetch_bountycaster(client)
        ),
        fetch_if_not_skipped(&skipped, "moltlaunch", sources::fetch_moltlaunch(client)),
        fetch_if_not_skipped(&skipped, "shillbot", sources::fetch_shillbot(client)),
        fetch_if_not_skipped(
            &skipped,
            "defillama-ai",
            sources::fetch_defillama_ai_agents(client)
        ),
    );

    let fetch_results = vec![botbounty, bountycaster, moltlaunch, shillbot, defillama];

    // Update backoff state from this cycle's results.
    {
        let mut bk = state.backoff.lock().await;
        let now = Utc::now();
        for result in &fetch_results {
            let was_parked = bk.skip_until.contains_key(&result.source);
            apply_backoff_update(&mut bk, result, now);
            let is_parked = bk.skip_until.contains_key(&result.source);
            if !was_parked && is_parked {
                tracing::warn!(
                    source = %result.source,
                    "source parked in backoff"
                );
            }
        }
    }

    // Load ingestion config (fallback to defaults if not in Firestore)
    let config = load_ingestion_config(&state.db).await;

    // Collect all raw listings and deduplicate
    let mut seen = std::collections::HashSet::new();
    let mut all_raw: Vec<RawListing> = Vec::new();
    for result in &fetch_results {
        for listing in &result.listings {
            let key = listing.doc_id();
            if seen.insert(key) {
                all_raw.push(listing.clone());
            }
        }
    }

    tracing::info!(
        total_fetched = all_raw.len(),
        "fetched listings from external sources"
    );

    // Load existing listings from Firestore for diffing
    let existing = load_existing_listings(&state.db).await;

    // Process each listing: filter, upsert, emit events
    let now = Utc::now();
    let mut active_doc_ids = std::collections::HashSet::new();
    let mut result_listings: Vec<ListingDoc> = Vec::new();

    for raw in &all_raw {
        let filter_result = filters::apply_filters(raw, &config);
        let doc_id = raw.doc_id();
        active_doc_ids.insert(doc_id.clone());

        let doc = if let Some(existing_doc) = existing.get(&doc_id) {
            // Existing listing: update last_seen_at, possibly reappear
            let mut updated = existing_doc.clone();
            updated.last_seen_at = now;
            updated.filtered = filter_result.filtered;
            updated.filter_reason = filter_result.reason;
            // Update fields that may have changed at source
            updated.title.clone_from(&raw.title);
            updated.description.clone_from(&raw.description);
            updated.reward_amount.clone_from(&raw.reward_amount);
            updated.reward_usd_estimate = raw.reward_usd_estimate;

            if updated.status == "disappeared" {
                updated.status = "open".to_string();
                updated.disappeared_at = None;
                emit_event(&state.db, &doc_id, "reappeared", None, Some("open")).await;
            }

            updated
        } else {
            // New listing
            let doc = ListingDoc {
                source: raw.source.clone(),
                source_id: raw.source_id.clone(),
                source_url: raw.source_url.clone(),
                title: raw.title.clone(),
                description: raw.description.clone(),
                category: raw.category.clone(),
                tags: raw.tags.clone(),
                reward_amount: raw.reward_amount.clone(),
                reward_token: raw.reward_token.clone(),
                reward_chain: raw.reward_chain.clone(),
                reward_usd_estimate: raw.reward_usd_estimate,
                payment_model: raw.payment_model.clone(),
                escrow: raw.escrow,
                posted_at: raw.posted_at,
                deadline: raw.deadline,
                status: "open".to_string(),
                first_seen_at: now,
                last_seen_at: now,
                disappeared_at: None,
                filtered: filter_result.filtered,
                filter_reason: filter_result.reason,
            };
            emit_event(&state.db, &doc_id, "first_seen", None, None).await;
            doc
        };

        // Upsert to Firestore
        upsert_listing(&state.db, &doc).await;

        if !doc.filtered && doc.status == "open" {
            result_listings.push(doc);
        }
    }

    // Mark disappeared listings — but ONLY for sources that actually
    // succeeded this cycle. If moltlaunch returned 429 and we parsed its
    // empty response, we must not interpret "moltlaunch had no listings this
    // round" as "every moltlaunch listing disappeared." Previously we did,
    // and a single Cloudflare block would wipe all known moltlaunch listings
    // from the swarm.tips frontend.
    let successful_sources: HashSet<&str> = fetch_results
        .iter()
        .filter(|r| is_fetch_success(r))
        .map(|r| r.source.as_str())
        .collect();

    for (doc_id, existing_doc) in &existing {
        if existing_doc.status != "open" {
            continue;
        }
        if active_doc_ids.contains(doc_id) {
            continue;
        }
        // Guard: only sweep listings whose source actually responded 2xx
        // this cycle. Failed/skipped sources keep their last-known state.
        if !successful_sources.contains(existing_doc.source.as_str()) {
            continue;
        }
        let mut disappeared = existing_doc.clone();
        disappeared.status = "disappeared".to_string();
        disappeared.disappeared_at = Some(now);
        upsert_listing(&state.db, &disappeared).await;
        emit_event(&state.db, doc_id, "disappeared", Some("open"), None).await;
    }

    // Include last-known listings from sources that failed this cycle so the
    // frontend keeps showing them instead of going silent on a single 429.
    for (doc_id, existing_doc) in &existing {
        if existing_doc.status != "open" {
            continue;
        }
        if active_doc_ids.contains(doc_id) {
            continue;
        }
        if successful_sources.contains(existing_doc.source.as_str()) {
            continue;
        }
        if !existing_doc.filtered {
            result_listings.push(existing_doc.clone());
        }
    }

    // Record source health
    for result in &fetch_results {
        record_source_health(&state.db, &result.source, &result.health).await;
    }

    // Sort: most recent first
    result_listings.sort_by(|a, b| b.posted_at.cmp(&a.posted_at));

    let agent_jobs: Vec<AgentJob> = result_listings.iter().map(AgentJob::from).collect();

    // Update cache
    {
        let mut cache = state.cache.lock().await;
        *cache = Some(ListingsCache {
            listings: agent_jobs.clone(),
            fetched_at: now,
        });
    }

    tracing::info!(returned = agent_jobs.len(), "listings ingestion complete");

    Ok(agent_jobs)
}

// -- Firestore helpers --

async fn load_ingestion_config(db: &FirestoreDb) -> IngestionConfig {
    match db
        .fluent()
        .select()
        .by_id_in(INGESTION_CONFIG)
        .obj::<IngestionConfig>()
        .one("default")
        .await
    {
        Ok(Some(config)) => config,
        Ok(None) => {
            tracing::info!("no ingestion_config/default in Firestore, using defaults");
            IngestionConfig::default()
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to load ingestion config, using defaults");
            IngestionConfig::default()
        }
    }
}

/// Wrap a fetch future so sources currently in backoff get skipped without
/// incurring a network call. Returns a synthetic `FetchResult` the main
/// pipeline can identify via `error == Some("skipped_backoff")`.
async fn fetch_if_not_skipped<F>(skipped: &HashSet<String>, source: &str, fut: F) -> FetchResult
where
    F: std::future::Future<Output = FetchResult>,
{
    if skipped.contains(source) {
        return FetchResult {
            source: source.to_string(),
            listings: vec![],
            health: HealthCheck {
                timestamp: Utc::now(),
                status_code: 0,
                response_ms: 0,
                listing_count: 0,
                error: Some("skipped_backoff".to_string()),
            },
        };
    }
    fut.await
}

async fn load_existing_listings(db: &FirestoreDb) -> HashMap<String, ListingDoc> {
    match db
        .fluent()
        .select()
        .from(LISTINGS)
        .obj::<ListingDoc>()
        .query()
        .await
    {
        Ok(docs) => docs.into_iter().map(|d| (d.doc_id(), d)).collect(),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load existing listings");
            HashMap::new()
        }
    }
}

async fn upsert_listing(db: &FirestoreDb, doc: &ListingDoc) {
    let doc_id = doc.doc_id();
    if let Err(e) = db
        .fluent()
        .update()
        .in_col(LISTINGS)
        .document_id(&doc_id)
        .object(doc)
        .execute::<ListingDoc>()
        .await
    {
        tracing::warn!(doc_id = %doc_id, error = %e, "failed to upsert listing");
    }
}

async fn emit_event(
    db: &FirestoreDb,
    listing_id: &str,
    event_type: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
) {
    let event = ListingEventDoc {
        listing_id: listing_id.to_string(),
        event_type: event_type.to_string(),
        old_value: old_value.map(String::from),
        new_value: new_value.map(String::from),
        timestamp: Utc::now(),
    };

    let event_id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = db
        .fluent()
        .insert()
        .into(LISTING_EVENTS)
        .document_id(&event_id)
        .object(&event)
        .execute::<ListingEventDoc>()
        .await
    {
        tracing::warn!(
            listing_id = %listing_id,
            event_type = %event_type,
            error = %e,
            "failed to emit listing event"
        );
    }
}

async fn record_source_health(db: &FirestoreDb, source: &str, check: &HealthCheck) {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let doc_id = format!("{source}:{date}");

    // Try to load existing doc and append, or create new
    let mut health_doc = match db
        .fluent()
        .select()
        .by_id_in(SOURCE_HEALTH)
        .obj::<SourceHealthDoc>()
        .one(&doc_id)
        .await
    {
        Ok(Some(existing)) => existing,
        _ => SourceHealthDoc {
            source: source.to_string(),
            date: date.clone(),
            checks: Vec::new(),
            total_checks: 0,
            successful_checks: 0,
        },
    };

    health_doc.checks.push(check.clone());
    health_doc.total_checks = health_doc.total_checks.saturating_add(1);
    if check.error.is_none() && check.status_code >= 200 && check.status_code < 300 {
        health_doc.successful_checks = health_doc.successful_checks.saturating_add(1);
    }

    if let Err(e) = db
        .fluent()
        .update()
        .in_col(SOURCE_HEALTH)
        .document_id(&doc_id)
        .object(&health_doc)
        .execute::<SourceHealthDoc>()
        .await
    {
        tracing::warn!(source = %source, error = %e, "failed to record source health");
    }
}

/// CORS headers attached to every /internal/listings response.
///
/// The swarm.tips frontend fetches this endpoint directly from the browser
/// after first paint to refresh listings, so the response must be readable
/// from any origin.
pub const LISTINGS_CORS_HEADERS: [(&str, &str); 3] = [
    ("Access-Control-Allow-Origin", "*"),
    ("Access-Control-Allow-Methods", "GET, OPTIONS"),
    ("Access-Control-Max-Age", "3600"),
];

/// Build the CORS preflight response for OPTIONS /internal/listings.
pub fn listings_preflight_response() -> axum::http::Response<axum::body::Body> {
    let mut builder = axum::http::Response::builder();
    for (name, value) in LISTINGS_CORS_HEADERS {
        builder = builder.header(name, value);
    }
    builder.body(axum::body::Body::empty()).unwrap()
}

/// Build the axum handler for GET /internal/listings.
pub fn listings_handler(state: Arc<ListingsState>) -> axum::routing::MethodRouter {
    axum::routing::get(move || {
        let state = state.clone();
        async move {
            match get_listings(&state).await {
                Ok(listings) => (LISTINGS_CORS_HEADERS, axum::Json(listings)).into_response(),
                Err(e) => {
                    tracing::error!(error = %e, "listings ingestion failed");
                    (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        LISTINGS_CORS_HEADERS,
                        format!("{{\"error\": \"{e}\"}}"),
                    )
                        .into_response()
                }
            }
        }
    })
    .options(|| async { listings_preflight_response() })
}

use axum::response::IntoResponse;

#[cfg(test)]
mod backoff_tests {
    use super::*;

    fn mk_result(source: &str, status: u16, err: Option<&str>) -> FetchResult {
        FetchResult {
            source: source.to_string(),
            listings: vec![],
            health: HealthCheck {
                timestamp: Utc::now(),
                status_code: status,
                response_ms: 0,
                listing_count: 0,
                error: err.map(String::from),
            },
        }
    }

    #[test]
    fn is_fetch_success_on_200_no_error() {
        assert!(is_fetch_success(&mk_result("s", 200, None)));
    }

    #[test]
    fn is_fetch_success_rejects_5xx_and_429() {
        assert!(!is_fetch_success(&mk_result("s", 429, None)));
        assert!(!is_fetch_success(&mk_result("s", 500, None)));
        assert!(!is_fetch_success(&mk_result("s", 503, None)));
    }

    #[test]
    fn is_fetch_success_rejects_error_even_with_200() {
        assert!(!is_fetch_success(&mk_result(
            "s",
            200,
            Some("decode failed"),
        )));
    }

    #[test]
    fn is_fetch_success_rejects_zero_status() {
        // Our synthetic "skipped_backoff" and transport-error cases set 0.
        assert!(!is_fetch_success(&mk_result(
            "s",
            0,
            Some("skipped_backoff")
        )));
        assert!(!is_fetch_success(&mk_result("s", 0, Some("timeout"))));
    }

    #[test]
    fn success_clears_failure_count_and_skip() {
        let mut bk = SourceBackoff::default();
        bk.consecutive_failures.insert("s".to_string(), 2);
        let now = Utc::now();
        apply_backoff_update(&mut bk, &mk_result("s", 200, None), now);
        assert!(!bk.consecutive_failures.contains_key("s"));
        assert!(!bk.skip_until.contains_key("s"));
    }

    #[test]
    fn two_failures_below_threshold_do_not_park() {
        let mut bk = SourceBackoff::default();
        let now = Utc::now();
        apply_backoff_update(&mut bk, &mk_result("s", 429, None), now);
        apply_backoff_update(&mut bk, &mk_result("s", 429, None), now);
        assert_eq!(bk.consecutive_failures.get("s"), Some(&2));
        assert!(!bk.skip_until.contains_key("s"));
    }

    #[test]
    fn third_failure_parks_for_backoff_window() {
        let mut bk = SourceBackoff::default();
        let now = Utc::now();
        for _ in 0..3 {
            apply_backoff_update(&mut bk, &mk_result("s", 429, None), now);
        }
        assert_eq!(bk.consecutive_failures.get("s"), Some(&3));
        let until = bk.skip_until.get("s").expect("should be parked");
        let delta = until.signed_duration_since(now).num_seconds();
        assert_eq!(delta, BACKOFF_WINDOW_SECS);
    }

    #[test]
    fn success_after_park_clears_park() {
        let mut bk = SourceBackoff::default();
        let now = Utc::now();
        for _ in 0..3 {
            apply_backoff_update(&mut bk, &mk_result("s", 429, None), now);
        }
        assert!(bk.skip_until.contains_key("s"));
        apply_backoff_update(&mut bk, &mk_result("s", 200, None), now);
        assert!(!bk.skip_until.contains_key("s"));
        assert!(!bk.consecutive_failures.contains_key("s"));
    }

    #[test]
    fn skipped_backoff_does_not_update_state() {
        let mut bk = SourceBackoff::default();
        bk.consecutive_failures.insert("s".to_string(), 2);
        let now = Utc::now();
        apply_backoff_update(&mut bk, &mk_result("s", 0, Some("skipped_backoff")), now);
        // failure count unchanged — the skip doesn't count as a failure OR
        // a success.
        assert_eq!(bk.consecutive_failures.get("s"), Some(&2));
    }

    #[test]
    fn backoff_per_source_is_independent() {
        let mut bk = SourceBackoff::default();
        let now = Utc::now();
        for _ in 0..3 {
            apply_backoff_update(&mut bk, &mk_result("a", 429, None), now);
        }
        apply_backoff_update(&mut bk, &mk_result("b", 200, None), now);
        assert!(bk.skip_until.contains_key("a"));
        assert!(!bk.skip_until.contains_key("b"));
    }
}

#[cfg(test)]
mod cors_tests {
    use super::*;

    #[test]
    fn preflight_response_has_cors_headers() {
        let resp = listings_preflight_response();
        let headers = resp.headers();
        assert_eq!(headers.get("access-control-allow-origin").unwrap(), "*");
        assert_eq!(
            headers.get("access-control-allow-methods").unwrap(),
            "GET, OPTIONS"
        );
        assert_eq!(headers.get("access-control-max-age").unwrap(), "3600");
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }

    #[test]
    fn cors_headers_constant_matches_browser_expectations() {
        // The swarm.tips client script (frontend/swarm-tips/src/lib/
        // listings-transform.ts) does a simple GET fetch with no custom
        // headers, so a wildcard Allow-Origin is sufficient. If we ever
        // add credentialed fetches we must echo the Origin instead of "*".
        let map: std::collections::HashMap<_, _> = LISTINGS_CORS_HEADERS.iter().copied().collect();
        assert_eq!(map.get("Access-Control-Allow-Origin"), Some(&"*"));
        assert!(map
            .get("Access-Control-Allow-Methods")
            .unwrap()
            .contains("GET"));
    }
}
