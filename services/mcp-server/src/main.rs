//! MCP Server — unified tool server for Swarm Tips.
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

// auth and session modules are used by Shillbot tools (hidden until mainnet).
// Remove #[allow(dead_code)] when re-enabling #[tool] attributes in server.rs.
#[allow(dead_code)]
mod auth;
mod config;
mod discovery;
mod errors;
mod game_proxy;
mod game_session;
mod listings;
mod proxy;
mod server;
mod session_binding;
mod solana_tx;

use crate::auth::ChallengeManager;
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let gcp_project_id = load_env_or("GCP_PROJECT_ID", "coordination-game-prod");
    let orchestrator_url = load_env_or("ORCHESTRATOR_URL", "http://shillbot-orchestrator:8080");
    let game_api_url = load_env_or("GAME_API_URL", "http://game-api:8080");
    // Prefer Secret Manager for the RPC URL (cross-repo standard: direct Secret
    // Manager reads for runtime secrets, never K8s Secrets as a bridge). Falls
    // back to SOLANA_RPC_URL env var for local dev, then to public devnet.
    let network = load_env_or("SOLANA_NETWORK", "mainnet");
    let rpc_secret = format!("solana-rpc-url-{network}");
    let solana_rpc_url = if let Some(url) =
        config::load_optional_secret(&gcp_project_id, &rpc_secret).await
    {
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
    };
    let host = load_env_or("HOST", DEFAULT_HOST);
    let port: u16 = load_env_or("PORT", &DEFAULT_PORT.to_string())
        .parse()
        .context("PORT must be a valid u16")?;

    tracing::info!(
        service = "mcp-server",
        orchestrator_url = %orchestrator_url,
        game_api_url = %game_api_url,
        host = %host,
        port = %port,
        "starting MCP server"
    );

    let challenge_manager = ChallengeManager::new();

    let rpc_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("reqwest client must build")?;

    // Firestore client for listings persistence
    let db = FirestoreDb::new(&gcp_project_id)
        .await
        .expect("Firestore client must initialize at startup");

    let listings_state = Arc::new(ListingsState::new(db));

    // Discovery (MCP mining engine) needs its own Firestore client + the
    // shared HTTP client. Best-effort: if Firestore init fails the discovery
    // routes will return empty data instead of crashing the whole server.
    let discovery_db = match FirestoreDb::new(&gcp_project_id).await {
        Ok(db) => Some(db),
        Err(e) => {
            tracing::error!(
                error = %e,
                "Firestore init for discovery failed — /internal/mcp/* will return empty"
            );
            None
        }
    };
    // Optional Layer 2 LLM classifier — enabled if `xai-api-key` is available
    // in GCP Secret Manager. Without it, refresh + earning-candidates work as
    // before; the /internal/mcp/llm-classify endpoint returns 503. Reads
    // directly from Secret Manager via Workload Identity — never from K8s
    // Secrets or env vars. See `config.rs` + the "Three secret categories,
    // three homes" rule in `swarm/CLAUDE.md`.
    let xai_api_key = config::load_optional_secret(&gcp_project_id, "xai-api-key").await;
    if xai_api_key.is_none() {
        tracing::warn!(
            service = "mcp-server",
            "xai-api-key not found in Secret Manager — Layer 2 LLM classification disabled"
        );
    }
    let llm_classifier = xai_api_key
        .map(|key| crate::discovery::llm_classify::LlmClassifier::new(key, rpc_client.clone()));

    let discovery_state = discovery_db
        .map(|db| Arc::new(DiscoveryState::new(db, rpc_client.clone(), llm_classifier)));

    // Second Firestore client for game session persistence (cheap client wrapper).
    let game_db = FirestoreDb::new(&gcp_project_id)
        .await
        .expect("Firestore client for game sessions must initialize");
    let game_db = Arc::new(game_db);
    let game_sessions = Arc::new(GameSessionManager::new(
        game_api_url.clone(),
        solana_rpc_url.clone(),
        Arc::clone(&game_db),
    ));

    // MCP HTTP session binding — `Mcp-Session-Id → wallet` lookup so a pod
    // restart doesn't strand an active agent. Shares the game-session
    // Firestore client because both are cheap wrappers around the same
    // underlying connection pool.
    let session_binding = Arc::new(McpSessionBinding::new(game_db));

    let rpc_url_for_verify = solana_rpc_url.clone();
    let shared = Arc::new(SharedState {
        orchestrator: OrchestratorProxy::new(orchestrator_url),
        game_api: GameApiProxy::new(game_api_url)?,
        solana_rpc_url,
        rpc_client,
        game_sessions,
        challenge_manager,
        session_binding,
        listings: Arc::clone(&listings_state),
    });

    let ct = tokio_util::sync::CancellationToken::new();

    let service = StreamableHttpService::new(
        move || Ok(SwarmTipsMcp::new(shared.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(ct.child_token()),
    );

    let health_rpc_url = load_env_or("SOLANA_RPC_URL", "https://api.devnet.solana.com");
    let health_game_url = load_env_or("GAME_API_URL", "http://game-api:8080");
    let started_at = std::time::Instant::now();
    let health_handler = move || {
        let rpc_url = health_rpc_url.clone();
        let game_url = health_game_url.clone();
        async move {
            // 60s grace period for Autopilot WI token warmup
            if started_at.elapsed() < std::time::Duration::from_secs(60) {
                return (axum::http::StatusCode::OK, "ok (startup grace)");
            }

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_default();

            // Check game-api
            let game_ok = client
                .get(format!("{game_url}/health"))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);

            // Check Solana RPC
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

            if game_ok && rpc_ok {
                (axum::http::StatusCode::OK, "ok")
            } else if !game_ok {
                (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "game-api unreachable",
                )
            } else {
                (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "solana rpc unreachable",
                )
            }
        }
    };

    let mut router = axum::Router::new()
        .route("/health", axum::routing::get(health_handler))
        .route(
            "/internal/listings",
            listings::listings_handler(listings_state),
        )
        .route(
            "/internal/build-verify-tx",
            axum::routing::post(move |body: axum::Json<serde_json::Value>| async move {
                build_verify_tx_handler(body, &rpc_url_for_verify).await
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
        );

    if let Some(discovery_state) = discovery_state {
        router = router
            .route(
                "/internal/mcp/earning-candidates",
                discovery::earning_candidates_handler(discovery_state.clone()),
            )
            .route(
                "/internal/mcp/primitives",
                discovery::primitives_handler(discovery_state.clone()),
            )
            .route(
                "/internal/mcp/refresh",
                discovery::refresh_handler(discovery_state.clone()),
            )
            .route(
                "/internal/mcp/llm-classify",
                discovery::llm_classify_handler(discovery_state.clone()),
            )
            .route(
                "/internal/mcp/deep-analyze",
                discovery::deep_analyze_handler(discovery_state),
            );
    }

    let router = router.nest_service("/mcp", service);
    let bind_addr = format!("{host}:{port}");
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
