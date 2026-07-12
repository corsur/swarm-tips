//! `/internal/traffic-stats` — the single aggregation point behind
//! swarm.tips/pulse.
//!
//! Merges the two siloed observability systems into one JSON response:
//!
//! ```text
//! Umami share-token API ──┐   (per-site pageviews, visitors, referrers,
//! (3 product frontends)   │    @corsur/analytics custom events)
//!                         ├──► GET /internal/traffic-stats ──► swarm.tips/pulse
//! Cloud Monitoring API ───┘   (swarm_funnel_* log-based metrics,
//! (agent/on-chain funnel)      per-cohort splits, unique agents)
//! ```
//!
//! Both sources degrade independently: a failure on one side returns
//! `available: false` for that section (logged at WARN) while the other
//! side still serves — same posture as `get_listings`.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use axum::response::IntoResponse;
use chrono::Utc;
use gcloud_sdk::google::monitoring::v3::{
    aggregation::Aligner, list_time_series_request::TimeSeriesView,
    metric_service_client::MetricServiceClient, typed_value, Aggregation, ListTimeSeriesRequest,
    TimeInterval,
};
use gcloud_sdk::GoogleApi;
use serde_json::{json, Value};
use tokio::sync::Mutex;

const FUNNEL_METRIC_PREFIX: &str = "logging.googleapis.com/user/swarm_funnel_";
const REGISTRATIONS_METRIC: &str = "logging.googleapis.com/user/mcp_agent_registrations";
/// (label, seconds) for the two always-shown windows.
const WINDOWS: [(&str, i64); 2] = [("24h", 86_400), ("7d", 604_800)];
const CACHE_TTL_SECS: i64 = 300;
/// Umami metrics lists (referrers, events) are capped server-side; the page
/// shows at most this many rows per list.
const UMAMI_METRICS_LIMIT: u32 = 10;

const CORS_HEADERS: [(&str, &str); 1] = [("Access-Control-Allow-Origin", "*")];

struct CachedStats {
    fetched_at: chrono::DateTime<Utc>,
    body: Value,
}

pub struct TrafficStatsState {
    http: reqwest::Client,
    gcp_project_id: String,
    umami_base_url: String,
    /// domain -> Umami share id, from the `umami-share-tokens` secret.
    /// `None` when the secret is absent — web section degrades.
    share_tokens: Option<HashMap<String, String>>,
    cache: Mutex<Option<CachedStats>>,
}

impl TrafficStatsState {
    pub fn new(
        http: reqwest::Client,
        gcp_project_id: String,
        umami_base_url: String,
        share_tokens_json: Option<String>,
    ) -> Self {
        let share_tokens = share_tokens_json.as_deref().and_then(parse_share_tokens);
        if share_tokens.is_none() {
            tracing::warn!(
                service = "mcp-server",
                "umami-share-tokens secret absent or unparseable — /internal/traffic-stats will serve funnel data only"
            );
        }
        Self {
            http,
            gcp_project_id,
            umami_base_url,
            share_tokens,
            cache: Mutex::new(None),
        }
    }
}

/// Parse the `umami-share-tokens` secret: a JSON object of
/// `{"<domain>": "<share id>"}`. Returns `None` (never panics) on any shape
/// mismatch so a bad secret degrades instead of crash-looping the pod.
fn parse_share_tokens(raw: &str) -> Option<HashMap<String, String>> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let obj = value.as_object()?;
    if obj.is_empty() {
        return None;
    }
    let mut map = HashMap::new();
    for (domain, token) in obj {
        map.insert(domain.clone(), token.as_str()?.to_string());
    }
    Some(map)
}

pub fn traffic_stats_handler(state: Arc<TrafficStatsState>) -> axum::routing::MethodRouter {
    axum::routing::get(move || {
        let state = state.clone();
        async move {
            let body = get_traffic_stats(&state).await;
            (CORS_HEADERS, axum::Json(body)).into_response()
        }
    })
    .options(|| async {
        axum::http::Response::builder()
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "GET, OPTIONS")
            .header("Access-Control-Allow-Headers", "content-type")
            .header("Access-Control-Max-Age", "3600")
            .body(axum::body::Body::empty())
            .unwrap()
    })
}

async fn get_traffic_stats(state: &TrafficStatsState) -> Value {
    {
        let cache = state.cache.lock().await;
        if let Some(cached) = cache.as_ref() {
            let age = Utc::now()
                .signed_duration_since(cached.fetched_at)
                .num_seconds();
            if age < CACHE_TTL_SECS {
                return cached.body.clone();
            }
        }
    }

    let (web, funnel) = tokio::join!(fetch_web_section(state), fetch_funnel_section(state));

    let body = json!({
        "generated_at": Utc::now().to_rfc3339(),
        "windows": WINDOWS.iter().map(|(label, _)| *label).collect::<Vec<_>>(),
        "web": web,
        "funnel": funnel,
    });

    let mut cache = state.cache.lock().await;
    *cache = Some(CachedStats {
        fetched_at: Utc::now(),
        body: body.clone(),
    });
    body
}

// ---------------------------------------------------------------------------
// Web side — Umami share-token API
// ---------------------------------------------------------------------------

async fn fetch_web_section(state: &TrafficStatsState) -> Value {
    let Some(tokens) = state.share_tokens.as_ref() else {
        return json!({ "available": false, "reason": "umami-share-tokens secret not configured" });
    };

    // Sites render in a stable order regardless of secret key order.
    let mut domains: Vec<&String> = tokens.keys().collect();
    domains.sort();

    let mut sites = Vec::new();
    let mut any_ok = false;
    for domain in domains {
        match fetch_site(state, domain, &tokens[domain]).await {
            Ok(site) => {
                any_ok = true;
                sites.push(site);
            }
            Err(e) => {
                tracing::warn!(
                    service = "mcp-server",
                    site = %domain,
                    error = %e,
                    "umami fetch failed for site"
                );
                sites.push(json!({ "domain": domain, "available": false }));
            }
        }
    }

    json!({ "available": any_ok, "sites": sites })
}

async fn fetch_site(
    state: &TrafficStatsState,
    domain: &str,
    share_id: &str,
) -> Result<Value, anyhow::Error> {
    // Share handshake: the share id resolves to the website id plus a
    // short-lived token that authenticates the stats endpoints.
    let share: Value = state
        .http
        .get(format!("{}/api/share/{share_id}", state.umami_base_url))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    // Umami v3 field name; older releases used `id`.
    let website_id = share["websiteId"]
        .as_str()
        .or_else(|| share["id"].as_str())
        .ok_or_else(|| anyhow::anyhow!("share response missing website id"))?
        .to_string();
    let share_token = share["token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("share response missing token"))?
        .to_string();

    let now_ms = Utc::now().timestamp_millis();
    let mut windows = serde_json::Map::new();
    for (label, secs) in WINDOWS {
        let start_ms = now_ms.saturating_sub(secs.saturating_mul(1000));
        let base = format!("{}/api/websites/{website_id}", state.umami_base_url);
        let range = [
            ("startAt", start_ms.to_string()),
            ("endAt", now_ms.to_string()),
        ];

        let stats: Value = umami_get(state, &share_token, &format!("{base}/stats"), &range).await?;
        let referrers: Value = umami_get(
            state,
            &share_token,
            &format!("{base}/metrics"),
            &[
                ("type", "referrer".to_string()),
                ("startAt", start_ms.to_string()),
                ("endAt", now_ms.to_string()),
                ("limit", UMAMI_METRICS_LIMIT.to_string()),
            ],
        )
        .await?;
        let events: Value = umami_get(
            state,
            &share_token,
            &format!("{base}/metrics"),
            &[
                ("type", "event".to_string()),
                ("startAt", start_ms.to_string()),
                ("endAt", now_ms.to_string()),
                ("limit", UMAMI_METRICS_LIMIT.to_string()),
            ],
        )
        .await?;

        windows.insert(
            label.to_string(),
            shape_site_window(&stats, &referrers, &events),
        );
    }

    Ok(json!({ "domain": domain, "available": true, "windows": windows }))
}

async fn umami_get(
    state: &TrafficStatsState,
    share_token: &str,
    url: &str,
    query: &[(&str, String)],
) -> Result<Value, anyhow::Error> {
    // v3 rejects share tokens presented outside a "share context" — the
    // context header is a literal "1" (mirrors useApi.ts in umami source).
    Ok(state
        .http
        .get(url)
        .header("x-umami-share-token", share_token)
        .header("x-umami-share-context", "1")
        .query(query)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// Shape one site window from the three raw Umami responses. Pure — unit
/// tested against captured response shapes.
fn shape_site_window(stats: &Value, referrers: &Value, events: &Value) -> Value {
    // Umami v2 `/stats` values arrive as {"value": n, "prev": m}; older
    // deployments returned bare numbers. Accept both.
    let stat = |key: &str| -> i64 {
        let node = &stats[key];
        node["value"]
            .as_i64()
            .or_else(|| node.as_i64())
            .unwrap_or(0)
    };
    let name_counts = |metrics: &Value| -> Vec<Value> {
        metrics
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        json!({
                            "name": row["x"].as_str().unwrap_or("(none)"),
                            "count": row["y"].as_i64().unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    json!({
        "pageviews": stat("pageviews"),
        "visitors": stat("visitors"),
        "referrers": name_counts(referrers),
        "events": name_counts(events),
    })
}

// ---------------------------------------------------------------------------
// Funnel side — Cloud Monitoring log-based metrics
// ---------------------------------------------------------------------------

async fn fetch_funnel_section(state: &TrafficStatsState) -> Value {
    match fetch_funnel_inner(state).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                service = "mcp-server",
                error = %e,
                "cloud monitoring funnel fetch failed"
            );
            json!({ "available": false })
        }
    }
}

async fn fetch_funnel_inner(state: &TrafficStatsState) -> Result<Value, anyhow::Error> {
    let client: GoogleApi<MetricServiceClient<_>> = GoogleApi::from_function(
        MetricServiceClient::new,
        "https://monitoring.googleapis.com",
        None,
    )
    .await?;

    // ListTimeSeries only accepts ONE metric.type per request (prefix
    // filters are rejected there) — discover the swarm_funnel_* names via
    // the descriptors API, then query each metric individually.
    let descriptors = client
        .get()
        .list_metric_descriptors(
            gcloud_sdk::google::monitoring::v3::ListMetricDescriptorsRequest {
                name: format!("projects/{}", state.gcp_project_id),
                filter: format!("metric.type = starts_with(\"{FUNNEL_METRIC_PREFIX}\")"),
                ..Default::default()
            },
        )
        .await?
        .into_inner();
    let metric_types: Vec<String> = descriptors
        .metric_descriptors
        .into_iter()
        .map(|d| d.r#type)
        .collect();

    let mut windows = serde_json::Map::new();
    for (label, secs) in WINDOWS {
        let mut series = Vec::new();
        for metric_type in &metric_types {
            let filter = format!("metric.type = \"{metric_type}\"");
            let request = build_list_request(&state.gcp_project_id, &filter, secs);
            let response = client.get().list_time_series(request).await?.into_inner();
            series.extend(sum_series(response.time_series));
        }
        windows.insert(label.to_string(), Value::Object(summarize_funnel(series)));
    }

    // Unique agents over 7d: count distinct wallet labels on the
    // registrations metric (the one accepted high-cardinality label).
    let reg_filter = format!("metric.type = \"{REGISTRATIONS_METRIC}\"");
    let request = build_list_request(&state.gcp_project_id, &reg_filter, 604_800);
    let response = client.get().list_time_series(request).await?.into_inner();
    let unique_agents: std::collections::HashSet<String> = sum_series(response.time_series)
        .into_iter()
        .filter(|(_, _, total)| *total > 0)
        .filter_map(|(_, labels, _)| labels.get("wallet").cloned())
        .collect();

    Ok(json!({
        "available": true,
        "windows": windows,
        "unique_agents_7d": unique_agents.len(),
    }))
}

/// One summed series: (metric type, metric labels, total over the window).
type SummedSeries = (String, HashMap<String, String>, i64);

/// Build the ListTimeSeries request for one window. The window doubles as
/// the alignment period so ALIGN_DELTA yields one summed point per series.
/// Pure — unit tested.
fn build_list_request(project_id: &str, filter: &str, window_secs: i64) -> ListTimeSeriesRequest {
    let now = Utc::now().timestamp();
    ListTimeSeriesRequest {
        name: format!("projects/{project_id}"),
        filter: filter.to_string(),
        interval: Some(TimeInterval {
            start_time: Some(gcloud_sdk::prost_types::Timestamp {
                seconds: now.saturating_sub(window_secs),
                nanos: 0,
            }),
            end_time: Some(gcloud_sdk::prost_types::Timestamp {
                seconds: now,
                nanos: 0,
            }),
        }),
        aggregation: Some(Aggregation {
            alignment_period: Some(gcloud_sdk::prost_types::Duration {
                seconds: window_secs,
                nanos: 0,
            }),
            per_series_aligner: Aligner::AlignDelta as i32,
            ..Default::default()
        }),
        view: TimeSeriesView::Full as i32,
        ..Default::default()
    }
}

/// Sum every point of every returned series. Pure — unit tested.
fn sum_series(
    time_series: Vec<gcloud_sdk::google::monitoring::v3::TimeSeries>,
) -> Vec<SummedSeries> {
    let mut out = Vec::new();
    for ts in time_series {
        let Some(metric) = ts.metric else { continue };
        let total: i64 = ts
            .points
            .iter()
            .filter_map(|p| p.value.as_ref())
            .filter_map(|v| match v.value {
                Some(typed_value::Value::Int64Value(n)) => Some(n),
                Some(typed_value::Value::DoubleValue(d)) => Some(d as i64),
                _ => None,
            })
            .sum();
        out.push((metric.r#type, metric.labels, total));
    }
    out
}

/// Fold summed series into `{short_metric_name: {total, by_cohort?}}`. Pure —
/// unit tested. Series carrying a `cohort` label contribute to a per-cohort
/// breakdown alongside the total.
fn summarize_funnel(series: Vec<SummedSeries>) -> serde_json::Map<String, Value> {
    let mut totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut cohorts: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();

    for (metric_type, labels, total) in series {
        let short = metric_type
            .strip_prefix(FUNNEL_METRIC_PREFIX)
            .unwrap_or(&metric_type)
            .to_string();
        let running = totals.entry(short.clone()).or_insert(0);
        *running = running.saturating_add(total);
        if let Some(cohort) = labels.get("cohort") {
            let cohort_running = cohorts
                .entry(short)
                .or_default()
                .entry(cohort.clone())
                .or_insert(0);
            *cohort_running = cohort_running.saturating_add(total);
        }
    }

    let mut out = serde_json::Map::new();
    for (metric, total) in totals {
        let mut entry = serde_json::Map::new();
        entry.insert("total".to_string(), json!(total));
        if let Some(by_cohort) = cohorts.get(&metric) {
            entry.insert("by_cohort".to_string(), json!(by_cohort));
        }
        out.insert(metric, Value::Object(entry));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_share_tokens -------------------------------------------------

    #[test]
    fn share_tokens_parse_happy_path() {
        let raw = r#"{"swarm.tips": "abc123", "coordination.game": "def456"}"#;
        let map = parse_share_tokens(raw).expect("valid map must parse");
        assert_eq!(map.len(), 2);
        assert_eq!(map["swarm.tips"], "abc123");
    }

    #[test]
    fn share_tokens_reject_bad_shapes() {
        assert!(parse_share_tokens("not json").is_none());
        assert!(parse_share_tokens("{}").is_none());
        assert!(parse_share_tokens(r#"{"a": 1}"#).is_none());
        assert!(parse_share_tokens(r#"["a"]"#).is_none());
    }

    // -- shape_site_window ---------------------------------------------------

    #[test]
    fn site_window_shapes_umami_v2_responses() {
        let stats = serde_json::json!({
            "pageviews": {"value": 120, "prev": 80},
            "visitors": {"value": 40, "prev": 22},
        });
        let referrers = serde_json::json!([
            {"x": "news.ycombinator.com", "y": 25},
            {"x": "", "y": 10},
        ]);
        let events = serde_json::json!([{"x": "wallet_connected", "y": 7}]);

        let shaped = shape_site_window(&stats, &referrers, &events);
        assert_eq!(shaped["pageviews"], 120);
        assert_eq!(shaped["visitors"], 40);
        assert_eq!(shaped["referrers"][0]["name"], "news.ycombinator.com");
        assert_eq!(shaped["referrers"][0]["count"], 25);
        assert_eq!(shaped["events"][0]["name"], "wallet_connected");
        assert_eq!(shaped["events"][0]["count"], 7);
    }

    #[test]
    fn site_window_tolerates_bare_numbers_and_garbage() {
        // Older Umami returns bare numbers; a broken response returns junk.
        let stats = serde_json::json!({"pageviews": 5, "visitors": "junk"});
        let shaped = shape_site_window(&stats, &Value::Null, &Value::Null);
        assert_eq!(shaped["pageviews"], 5);
        assert_eq!(shaped["visitors"], 0);
        assert_eq!(shaped["referrers"], serde_json::json!([]));
        assert_eq!(shaped["events"], serde_json::json!([]));
    }

    // -- summarize_funnel ----------------------------------------------------

    fn series(metric: &str, labels: &[(&str, &str)], total: i64) -> SummedSeries {
        (
            metric.to_string(),
            labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            total,
        )
    }

    #[test]
    fn funnel_sums_across_series_and_splits_cohorts() {
        let rows = vec![
            series(
                "logging.googleapis.com/user/swarm_funnel_game_started",
                &[("cohort", "control")],
                3,
            ),
            series(
                "logging.googleapis.com/user/swarm_funnel_game_started",
                &[("cohort", "treatment_a")],
                5,
            ),
            series(
                "logging.googleapis.com/user/swarm_funnel_register_wallet_solana",
                &[],
                9,
            ),
        ];

        let out = summarize_funnel(rows);
        assert_eq!(out["game_started"]["total"], 8);
        assert_eq!(out["game_started"]["by_cohort"]["control"], 3);
        assert_eq!(out["game_started"]["by_cohort"]["treatment_a"], 5);
        assert_eq!(out["register_wallet_solana"]["total"], 9);
        assert!(out["register_wallet_solana"].get("by_cohort").is_none());
    }

    #[test]
    fn funnel_handles_empty_and_unprefixed_series() {
        assert!(summarize_funnel(vec![]).is_empty());
        // A metric outside the prefix keeps its full name rather than being
        // silently dropped — visible is better than missing.
        let out = summarize_funnel(vec![series("custom.metric/foo", &[], 2)]);
        assert_eq!(out["custom.metric/foo"]["total"], 2);
    }

    // -- monitoring request/response helpers ---------------------------------

    #[test]
    fn list_request_aligns_delta_over_full_window() {
        let req = build_list_request("proj-x", "metric.type = \"m\"", 86_400);
        assert_eq!(req.name, "projects/proj-x");
        let interval = req.interval.expect("interval set");
        let span =
            interval.end_time.expect("end").seconds - interval.start_time.expect("start").seconds;
        assert_eq!(span, 86_400);
        let agg = req.aggregation.expect("aggregation set");
        assert_eq!(agg.alignment_period.expect("period").seconds, 86_400);
        assert_eq!(agg.per_series_aligner, Aligner::AlignDelta as i32);
    }

    #[test]
    fn sum_series_totals_points_and_skips_metricless() {
        use gcloud_sdk::google::api::Metric;
        use gcloud_sdk::google::monitoring::v3::{Point, TimeSeries, TypedValue};

        let point = |n: i64| Point {
            value: Some(TypedValue {
                value: Some(typed_value::Value::Int64Value(n)),
            }),
            ..Default::default()
        };
        let series = vec![
            TimeSeries {
                metric: Some(Metric {
                    r#type: "logging.googleapis.com/user/swarm_funnel_game_started".into(),
                    labels: HashMap::from([("cohort".to_string(), "control".to_string())]),
                }),
                points: vec![point(2), point(3)],
                ..Default::default()
            },
            // No metric attached — must be skipped, not panicked on.
            TimeSeries::default(),
        ];

        let summed = sum_series(series);
        assert_eq!(summed.len(), 1);
        assert_eq!(summed[0].2, 5);
        assert_eq!(summed[0].1["cohort"], "control");
    }

    // -- degradation flow ----------------------------------------------------

    #[tokio::test]
    async fn web_section_unavailable_without_tokens() {
        let state = TrafficStatsState::new(
            reqwest::Client::new(),
            "test-project".to_string(),
            "http://umami.invalid".to_string(),
            None,
        );
        let web = fetch_web_section(&state).await;
        assert_eq!(web["available"], false);
    }

    #[tokio::test]
    async fn web_section_degrades_per_site_on_unreachable_umami() {
        // Unresolvable host: every site errors, section reports unavailable,
        // and each site is individually marked — the error path of the
        // fetch flow without any live network dependency in CI assertions.
        let state = TrafficStatsState::new(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(200))
                .build()
                .expect("client builds"),
            "test-project".to_string(),
            "http://umami-does-not-exist.invalid".to_string(),
            Some(r#"{"swarm.tips": "tok"}"#.to_string()),
        );
        let web = fetch_web_section(&state).await;
        assert_eq!(web["available"], false);
        assert_eq!(web["sites"][0]["domain"], "swarm.tips");
        assert_eq!(web["sites"][0]["available"], false);
    }
}
