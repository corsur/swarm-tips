//! MCP Server — unified tool server for Swarm Tips.
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

mod composite_trust;
mod config;
mod discovery;
mod errors;
mod game_proxy;
mod game_session;
mod listings;
#[cfg(test)]
mod matrix_tests;
mod proxy;
mod reputation;
mod server;
mod session_binding;
mod solana_reads;
mod solana_tx;
mod traffic_stats;
mod web_position;
mod xchain;

use crate::discovery::DiscoveryState;
use crate::game_proxy::GameApiProxy;
use crate::game_session::GameSessionManager;
use crate::listings::ListingsState;
use crate::proxy::OrchestratorProxy;
use crate::server::{SharedState, SwarmTipsMcp};
use crate::session_binding::McpSessionBinding;
use anyhow::Context;
use firestore::FirestoreDb;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::sync::Arc;

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_HOST: &str = "0.0.0.0";

fn load_env_or(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.to_string())
}

struct StartupConfig {
    gcp_project_id: String,
    orchestrator_url: String,
    shorts_api_url: String,
    game_api_url: String,
    solana_rpc_url: String,
    network: String,
    host: String,
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cfg = load_startup_config().await?;
    log_startup(&cfg);

    let rpc_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("reqwest client must build")?;

    let listings_state = Arc::new(ListingsState::new(
        open_firestore(&cfg.gcp_project_id).await,
        rpc_client.clone(),
    ));
    let discovery_state = build_discovery_state(&cfg.gcp_project_id, &rpc_client).await;

    let traffic_stats_state = Arc::new(traffic_stats::TrafficStatsState::new(
        rpc_client.clone(),
        cfg.gcp_project_id.clone(),
        load_env_or("UMAMI_BASE_URL", "https://analytics.coordination.game"),
        config::load_optional_secret(&cfg.gcp_project_id, "umami-share-tokens").await,
    ));

    let game_db = Arc::new(open_firestore(&cfg.gcp_project_id).await);
    let (rpc_url_mainnet, rpc_url_devnet) = load_per_network_rpcs(&cfg).await;
    let game_sessions = Arc::new(GameSessionManager::new(
        cfg.game_api_url.clone(),
        cfg.solana_rpc_url.clone(),
        rpc_url_mainnet.clone(),
        rpc_url_devnet.clone(),
        Arc::clone(&game_db),
    ));
    let session_binding = Arc::new(McpSessionBinding::new(game_db));

    let shared = Arc::new(SharedState {
        orchestrator: OrchestratorProxy::new(
            cfg.orchestrator_url.clone(),
            cfg.shorts_api_url.clone(),
        ),
        game_api: GameApiProxy::new(cfg.game_api_url.clone())?,
        solana_rpc_url: cfg.solana_rpc_url.clone(),
        solana_rpc_url_mainnet: rpc_url_mainnet.clone(),
        solana_rpc_url_devnet: rpc_url_devnet.clone(),
        rpc_client,
        game_sessions,
        session_binding,
        listings: Arc::clone(&listings_state),
        discovery: discovery_state.clone(),
    });

    let ct = tokio_util::sync::CancellationToken::new();
    // Stateless transport: the rmcp session is otherwise pod-local (in-memory
    // LocalSessionManager), so with KEDA min-2 a multi-step agent flow that
    // round-robins to the other pod 404s "Session not found". In stateless mode
    // each request is self-contained and any pod can serve it. The app's own
    // session state (wallet binding, game session) is already Firestore-backed
    // and re-hydrated per request in `resolve_wallet`, so no server-side session
    // is needed here. `json_response` returns plain JSON (POST-only; the SDK
    // tolerates the resulting 405 on its optional GET stream). The
    // `ensure_mcp_session_id` middleware below still threads an `Mcp-Session-Id`
    // so the Firestore wallet binding keeps a stable correlation key.
    let service = StreamableHttpService::new(
        move || Ok(SwarmTipsMcp::new(shared.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_cancellation_token(ct.child_token()),
    );

    let reputation_db = Arc::new(open_firestore(&cfg.gcp_project_id).await);
    // Shared internal key: /internal/reputation/rebuild is a service-to-service
    // webhook from shillbot-api's settlement-finalize path. Gate it on the same
    // shillbot-api-key the caller sends so it isn't a public unauthenticated
    // EigenTrust-recompute trigger. None → the route rejects all callers.
    let internal_api_key =
        config::load_optional_secret(&cfg.gcp_project_id, "shillbot-api-key").await;
    let router = build_router(
        listings_state,
        traffic_stats_state,
        discovery_state,
        reputation_db,
        internal_api_key,
        rpc_url_mainnet,
        rpc_url_devnet,
        service,
    );

    let bind_addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(service = "mcp-server", addr = %bind_addr, "MCP server ready");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}

async fn load_startup_config() -> anyhow::Result<StartupConfig> {
    let gcp_project_id = load_env_or("GCP_PROJECT_ID", "coordination-game-prod");
    let orchestrator_url = load_env_or("ORCHESTRATOR_URL", "http://shillbot-api:8080");
    // shorts-api was split out of shillbot-api; the in-cluster mcp-server must
    // call it directly for the /shorts/* (video) endpoints since it bypasses
    // the edge path-routing.
    let shorts_api_url = load_env_or("SHORTS_API_URL", "http://shorts-api");
    let game_api_url = load_env_or("GAME_API_URL", "http://game-api:8080");
    let network = load_env_or("SOLANA_NETWORK", "mainnet");
    let solana_rpc_url = load_solana_rpc_url(&gcp_project_id, &network).await;
    let host = load_env_or("HOST", DEFAULT_HOST);
    let port: u16 = load_env_or("PORT", &DEFAULT_PORT.to_string())
        .parse()
        .context("PORT must be a valid u16")?;
    Ok(StartupConfig {
        gcp_project_id,
        orchestrator_url,
        shorts_api_url,
        game_api_url,
        solana_rpc_url,
        network,
        host,
        port,
    })
}

/// Prefer Secret Manager for the RPC URL (cross-repo standard: direct
/// Secret Manager reads for runtime secrets, never K8s Secrets as a
/// bridge). Falls back to SOLANA_RPC_URL env var for local dev, then to
/// public devnet.
async fn load_solana_rpc_url(gcp_project_id: &str, network: &str) -> String {
    let rpc_secret = format!("solana-rpc-url-{network}");
    if let Some(url) = config::load_optional_secret(gcp_project_id, &rpc_secret).await {
        tracing::info!(service = "mcp-server", network = %network, "loaded Solana RPC URL from Secret Manager");
        url
    } else {
        let fallback = load_env_or("SOLANA_RPC_URL", "https://api.devnet.solana.com");
        tracing::warn!(
            service = "mcp-server",
            rpc_url = %fallback,
            "solana-rpc-url not in Secret Manager — falling back to env/devnet"
        );
        fallback
    }
}

fn log_startup(cfg: &StartupConfig) {
    tracing::info!(
        service = "mcp-server",
        orchestrator_url = %cfg.orchestrator_url,
        game_api_url = %cfg.game_api_url,
        host = %cfg.host,
        port = %cfg.port,
        "starting MCP server"
    );
}

async fn open_firestore(project_id: &str) -> FirestoreDb {
    FirestoreDb::new(project_id)
        .await
        .expect("Firestore client must initialize at startup")
}

/// Discovery (MCP mining engine) is best-effort: if Firestore init fails
/// the discovery routes return empty data rather than crashing the server.
/// Layer 2 LLM classifier is optional — enabled only if `xai-api-key` is
/// available in GCP Secret Manager (no env-var fallback per the
/// "Three secret categories, three homes" rule).
async fn build_discovery_state(
    gcp_project_id: &str,
    rpc_client: &reqwest::Client,
) -> Option<Arc<DiscoveryState>> {
    let discovery_db = match FirestoreDb::new(gcp_project_id).await {
        Ok(db) => Some(db),
        Err(e) => {
            tracing::error!(
                error = %e,
                "Firestore init for discovery failed — /internal/mcp/* will return empty"
            );
            None
        }
    };

    let xai_api_key = config::load_optional_secret(gcp_project_id, "xai-api-key").await;
    if xai_api_key.is_none() {
        tracing::warn!(
            service = "mcp-server",
            "xai-api-key not found in Secret Manager — Layer 2 LLM classification disabled"
        );
    }
    let llm_classifier = xai_api_key
        .map(|key| crate::discovery::llm_classify::LlmClassifier::new(key, rpc_client.clone()));

    let github_token = config::load_optional_secret(gcp_project_id, "github-token").await;
    if github_token.is_none() {
        tracing::warn!(
            service = "mcp-server",
            "github-token not found in Secret Manager — deep-analysis GitHub calls will use unauthenticated rate limits (60/hour/IP)"
        );
    }

    discovery_db.map(|db| {
        Arc::new(DiscoveryState::new(
            db,
            rpc_client.clone(),
            llm_classifier,
            github_token,
        ))
    })
}

/// Per-network RPC URLs for `/internal/build-verify-tx`: the request body
/// picks one (mainnet by default, devnet when `network: "devnet"` is set);
/// without the right URL the Switchboard feed lookup hits the wrong
/// cluster and fails.
async fn load_per_network_rpcs(cfg: &StartupConfig) -> (String, String) {
    let mainnet = if cfg.network == "mainnet" {
        cfg.solana_rpc_url.clone()
    } else {
        config::load_optional_secret(&cfg.gcp_project_id, "solana-rpc-url-mainnet")
            .await
            .unwrap_or_else(|| "https://api.mainnet-beta.solana.com".to_string())
    };
    let devnet = if cfg.network == "devnet" {
        cfg.solana_rpc_url.clone()
    } else {
        config::load_optional_secret(&cfg.gcp_project_id, "solana-rpc-url-devnet")
            .await
            .unwrap_or_else(|| "https://api.devnet.solana.com".to_string())
    };
    (mainnet, devnet)
}

// Router assembly wires many independent states/handles in one place; grouping
// them into a struct would just move the argument list, not shrink it.
#[allow(clippy::too_many_arguments)]
fn build_router(
    listings_state: Arc<ListingsState>,
    traffic_stats_state: Arc<traffic_stats::TrafficStatsState>,
    discovery_state: Option<Arc<DiscoveryState>>,
    reputation_db: Arc<firestore::FirestoreDb>,
    internal_api_key: Option<String>,
    rpc_url_mainnet: String,
    rpc_url_devnet: String,
    mcp_service: StreamableHttpService<SwarmTipsMcp, LocalSessionManager>,
) -> axum::Router {
    let mainnet_for_verify = rpc_url_mainnet.clone();
    let devnet_for_verify = rpc_url_devnet.clone();

    // KEDA scaling signal: in-flight request count across the whole server
    // (including the /mcp agent transport). mcp-server was a single fixed
    // replica — this + the ScaledObject let it scale under a spike so one
    // expensive call can't block every agent.
    let inflight = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut router = axum::Router::new()
        // Liveness: trivial — proves the process/event-loop is alive. It MUST NOT
        // make outbound calls: a slow game-api or Solana RPC must never get a
        // healthy pod killed. Depending on those here was the Exit-137 liveness
        // restart loop that periodically dropped every agent's streamable-http
        // session (the intermittent MCP auth failures).
        .route(
            "/health",
            axum::routing::get(|| async { (axum::http::StatusCode::OK, "ok") }),
        )
        // Human-facing root. swarm.tips links to this host in its nav, and the
        // MCP protocol endpoint lives at /mcp — without this route a browser
        // visit to the bare domain is an empty 404.
        .route(
            "/",
            axum::routing::get(|| async {
                (
                    [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    concat!(
                        "<!doctype html><html><head><meta charset=\"utf-8\">",
                        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
                        "<title>Swarm Tips MCP</title></head>",
                        "<body style=\"font-family:system-ui,sans-serif;max-width:40rem;",
                        "margin:4rem auto;padding:0 1rem;line-height:1.6\">",
                        "<h1>Swarm Tips MCP server</h1>",
                        "<p>This host is an MCP endpoint for AI agents, not a website. ",
                        "Connect an MCP client to <code>https://mcp.swarm.tips/mcp</code> ",
                        "(streamable HTTP).</p>",
                        "<p>For example: <code>claude mcp add --transport http swarm-tips ",
                        "https://mcp.swarm.tips/mcp</code></p>",
                        "<p>Quickstart and tool docs: ",
                        "<a href=\"https://swarm.tips/start\">swarm.tips/start</a> &middot; ",
                        "<a href=\"https://swarm.tips/developers\">swarm.tips/developers</a></p>",
                        "</body></html>"
                    ),
                )
            }),
        )
        // Readiness / observability: the game-api + Solana RPC dependency check.
        // Wired to the readiness probe only — a failing dependency drains traffic,
        // it never kills the process.
        .route(
            "/ready",
            axum::routing::get(build_readiness_handler(rpc_url_mainnet.clone())),
        )
        .route(
            "/internal/scaling-metric",
            axum::routing::get({
                let inflight = Arc::clone(&inflight);
                move || {
                    let value = inflight.load(std::sync::atomic::Ordering::Relaxed);
                    async move { axum::Json(serde_json::json!({ "value": value })) }
                }
            }),
        )
        .route(
            "/internal/listings",
            listings::listings_handler(listings_state),
        )
        .route(
            "/internal/traffic-stats",
            traffic_stats::traffic_stats_handler(traffic_stats_state),
        )
        .route(
            "/internal/build-verify-tx",
            axum::routing::post(move |body: axum::Json<serde_json::Value>| {
                let mainnet = mainnet_for_verify.clone();
                let devnet = devnet_for_verify.clone();
                async move {
                    let net = body["network"].as_str().unwrap_or("mainnet");
                    let rpc = if net == "devnet" { &devnet } else { &mainnet };
                    build_verify_tx_handler(body, rpc).await
                }
            })
            .options(|| async {
                // CORS preflight for browser requests from shillbot.org
                axum::http::Response::builder()
                    .header("Access-Control-Allow-Origin", "*")
                    .header("Access-Control-Allow-Methods", "POST, OPTIONS")
                    .header("Access-Control-Allow-Headers", "content-type")
                    .header("Access-Control-Max-Age", "3600")
                    .body(axum::body::Body::empty())
                    .unwrap()
            }),
        )
        .route(
            "/internal/reputation/rebuild",
            reputation::rebuild_handler(reputation_db.clone(), internal_api_key.clone()),
        )
        .route(
            "/internal/reputation/backfill",
            reputation::backfill_handler(
                Arc::clone(&reputation_db),
                reqwest::Client::new(),
                rpc_url_mainnet.clone(),
            ),
        )
        .route(
            // Settlement-graph leaderboard for the swarm.tips/reputation
            // page and the agent_reputation_leaderboard MCP tool.
            "/internal/reputation/leaderboard",
            reputation::leaderboard_handler(Arc::clone(&reputation_db)),
        )
        .route(
            // Plain-HTTP reputation read for the swarm.tips/reputation page
            // (B8). Two signals in one response: the extension-credit
            // web-position (on-chain read; network param, default mainnet
            // since the 2026-07-08 extension-registry mainnet deploy) and
            // the EigenTrust settlement-graph record (Firestore, mainnet
            // settlements — network-independent).
            "/internal/agent-reputation",
            axum::routing::get({
                let mainnet = rpc_url_mainnet.clone();
                let devnet = rpc_url_devnet.clone();
                let rep_db = Arc::clone(&reputation_db);
                move |q: axum::extract::Query<std::collections::HashMap<String, String>>| {
                    let mainnet = mainnet.clone();
                    let devnet = devnet.clone();
                    let rep_db = Arc::clone(&rep_db);
                    async move {
                        let wallet = q.get("wallet").cloned().unwrap_or_default();
                        let net = q.get("network").map(String::as_str).unwrap_or("mainnet");
                        let rpc = if net == "mainnet" { &mainnet } else { &devnet };
                        let client = reqwest::Client::new();
                        let (web_position, extensions_received) =
                            crate::web_position::agent_web_position(&client, rpc, &wallet).await;
                        let eigentrust = reputation::get_agent_reputation(&rep_db, &wallet).await;
                        (
                            [("Access-Control-Allow-Origin", "*")],
                            axum::Json(serde_json::json!({
                                "wallet": wallet,
                                "web_position": web_position,
                                "extensions_received": extensions_received,
                                "has_standing": web_position.is_some()
                                    && extensions_received >= 1,
                                "eigentrust": eigentrust,
                            })),
                        )
                    }
                }
            }),
        );

    if let Some(state) = discovery_state {
        router = router
            .route(
                "/internal/mcp/earning-candidates",
                discovery::earning_candidates_handler(state.clone()),
            )
            .route(
                "/internal/mcp/primitives",
                discovery::primitives_handler(state.clone()),
            )
            .route(
                "/internal/mcp/search",
                discovery::search_handler(state.clone()),
            )
            .route(
                "/internal/mcp/refresh",
                discovery::refresh_handler(state.clone()),
            )
            .route(
                "/internal/mcp/llm-classify",
                discovery::llm_classify_handler(state.clone()),
            )
            .route(
                "/internal/mcp/deep-analyze",
                discovery::deep_analyze_handler(state),
            );
    }

    // Apply body limit to the internal HTTP surface ONLY. The MCP
    // streamable-http transport at /mcp manages its own framing; layering
    // a body limit before nest_service excludes it from the cap.
    router
        .layer(axum::extract::DefaultBodyLimit::max(1_048_576))
        .nest_service("/mcp", mcp_service)
        // In-flight counter wraps EVERYTHING (incl. /mcp) so the scaling
        // metric reflects real agent load, not just the internal API.
        .layer(axum::middleware::from_fn_with_state(
            inflight,
            track_inflight,
        ))
        // Thread a stable Mcp-Session-Id so the Firestore wallet binding has a
        // cross-pod correlation key even though the transport is stateless.
        .layer(axum::middleware::from_fn(ensure_mcp_session_id))
}

/// Ensure every `/mcp` response carries an `Mcp-Session-Id`. In stateless rmcp
/// mode the transport mints none, but the Firestore wallet binding
/// (`session_binding.rs`) and `resolve_wallet` key on this header. So we thread
/// a stable id: echo the client's if present, otherwise mint one on the first
/// (`initialize`) call. A spec-compliant MCP client captures the header from the
/// response and echoes it on every later request, giving cross-pod-stable wallet
/// correlation without any pod-local session state.
async fn ensure_mcp_session_id(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let header = axum::http::HeaderName::from_static("mcp-session-id");
    let is_mcp = req.uri().path().starts_with("/mcp");
    let client_id = req.headers().get(&header).cloned();
    let mut resp = next.run(req).await;
    if is_mcp && !resp.headers().contains_key(&header) {
        let value = client_id
            .or_else(|| axum::http::HeaderValue::from_str(&uuid::Uuid::new_v4().to_string()).ok());
        if let Some(value) = value {
            resp.headers_mut().insert(header, value);
        }
    }
    resp
}

/// Increments the in-flight counter for the duration of each request. The
/// guard decrements on drop so the count is correct even if the handler
/// panics or the client disconnects mid-response.
async fn track_inflight(
    axum::extract::State(inflight): axum::extract::State<Arc<std::sync::atomic::AtomicUsize>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    inflight.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    struct Guard(Arc<std::sync::atomic::AtomicUsize>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let _guard = Guard(inflight);
    next.run(req).await
}

/// Readiness / dependency probe — gates ONLY on Solana RPC reachability.
/// game-api is probed for observability but never fails readiness: game-api
/// scales to zero (KEDA HTTP add-on) and in-cluster DNS bypasses the wake
/// interceptor, so "connection refused" is a NORMAL state — gating on it takes
/// all 51 tools offline to guard the ~10 game tools, which surface game-api
/// errors per-call anyway (2026-07-24 outage: stuck NotReady rollout).
/// Wired to the readiness probe and `/ready` ONLY, never liveness: a failing
/// dependency should drain traffic from this pod, never kill the (healthy) process.
///
/// `health_rpc_url` is the RESOLVED mainnet RPC the service actually serves with
/// (Secret Manager, via `load_solana_rpc_url`) — not a re-read of `SOLANA_RPC_URL`,
/// which would let readiness gate on a different endpoint than the one in use.
fn build_readiness_handler(
    health_rpc_url: String,
) -> impl Fn() -> std::pin::Pin<
    Box<dyn std::future::Future<Output = (axum::http::StatusCode, &'static str)> + Send + 'static>,
> + Clone
       + Send
       + 'static {
    let health_game_url = load_env_or("GAME_API_URL", "http://game-api:8080");
    let started_at = std::time::Instant::now();
    move || {
        let rpc_url = health_rpc_url.clone();
        let game_url = health_game_url.clone();
        Box::pin(async move {
            // 60s grace period for Autopilot WI token warmup
            if started_at.elapsed() < std::time::Duration::from_secs(60) {
                return (axum::http::StatusCode::OK, "ok (startup grace)");
            }

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()
                .unwrap_or_default();

            let game_ok = client
                .get(format!("{game_url}/health"))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);

            let rpc_ok = client
                .post(&rpc_url)
                .json(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "getHealth"
                }))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);

            if !game_ok {
                tracing::warn!(
                    service = "mcp-server",
                    game_api = %game_url,
                    "game-api unreachable from readiness probe (expected while scaled to zero)"
                );
            }

            if rpc_ok {
                (axum::http::StatusCode::OK, "ok")
            } else {
                (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "solana rpc unreachable",
                )
            }
        })
    }
}

/// HTTP handler for `/internal/build-verify-tx`.
///
/// Spawns `build-verify-tx.ts` server-side (no CORS issues with the
/// Switchboard gateway) and returns the unsigned bundled tx.
async fn build_verify_tx_handler(
    axum::Json(body): axum::Json<serde_json::Value>,
    rpc_url: &str,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let get = |key: &str| body[key].as_str().unwrap_or("").to_string();
    let task_id = get("task_id");
    let payer = get("payer");
    let score = body["score"].as_u64().unwrap_or(0).to_string();
    let hash = get("hash");
    let task_pda = get("task_pda");
    let feed = get("feed");
    let global_state = get("global_state");

    if task_id.is_empty() || payer.is_empty() || task_pda.is_empty() {
        return (StatusCode::BAD_REQUEST, "missing required fields").into_response();
    }

    let script_path = std::env::var("BUILD_VERIFY_SCRIPT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts")
                .join("build-verify-tx.ts")
        });
    let script_dir = script_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let network_str = body["network"].as_str().unwrap_or("mainnet");
    let output = tokio::process::Command::new("tsx")
        .current_dir(script_dir)
        .arg(&script_path)
        .arg("--task-id")
        .arg(&task_id)
        .arg("--payer")
        .arg(&payer)
        .arg("--score")
        .arg(&score)
        .arg("--hash")
        .arg(&hash)
        .arg("--task-pda")
        .arg(&task_pda)
        .arg("--feed")
        .arg(&feed)
        .arg("--global-state")
        .arg(&global_state)
        .arg("--rpc")
        .arg(rpc_url)
        .arg("--network")
        .arg(network_str)
        .output()
        .await;

    let cors_headers = [("Access-Control-Allow-Origin", "*")];

    match output {
        Ok(out) if out.status.success() => {
            let tx = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (
                cors_headers,
                axum::Json(serde_json::json!({ "transaction": tx })),
            )
                .into_response()
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::error!(service = "mcp-server", stderr = %stderr, "build-verify-tx failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                cors_headers,
                format!("build-verify-tx: {stderr}"),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(service = "mcp-server", error = %e, "spawn failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                cors_headers,
                format!("spawn: {e}"),
            )
                .into_response()
        }
    }
}
