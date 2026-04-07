//! MCP Server — unified tool server for Swarm Tips.
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

// auth and session modules are used by Shillbot tools (hidden until mainnet).
// Remove #[allow(dead_code)] when re-enabling #[tool] attributes in server.rs.
#[allow(dead_code)]
mod auth;
mod botbounty_proxy;
mod clawtasks_proxy;
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
use crate::botbounty_proxy::BotBountyProxy;
use crate::clawtasks_proxy::ClawTasksProxy;
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

    let orchestrator_url = load_env_or("ORCHESTRATOR_URL", "http://shillbot-orchestrator:8080");
    let game_api_url = load_env_or("GAME_API_URL", "http://game-api:8080");
    // Prefer Helius over public devnet endpoint for reliability + rate limits.
    // ConfigMap should set SOLANA_RPC_URL; falls back to public devnet only for local dev.
    let solana_rpc_url = load_env_or("SOLANA_RPC_URL", "https://api.devnet.solana.com");
    if solana_rpc_url.contains("api.devnet.solana.com")
        || solana_rpc_url.contains("api.mainnet-beta.solana.com")
    {
        tracing::warn!(
            service = "mcp-server",
            rpc_url = %solana_rpc_url,
            "using public Solana RPC — set SOLANA_RPC_URL to Helius for production reliability"
        );
    }
    let clawtasks_url = load_env_or("CLAWTASKS_API_URL", "https://clawtasks.com/api");
    let botbounty_url = load_env_or(
        "BOTBOUNTY_API_URL",
        "https://botbounty-production.up.railway.app/api",
    );
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

    let gcp_project_id = load_env_or("GCP_PROJECT_ID", "coordination-game-prod");

    let challenge_manager = ChallengeManager::new();

    let rpc_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("reqwest client must build")?;

    // Firestore client for listings persistence
    let db = FirestoreDb::new(&gcp_project_id)
        .await
        .expect("Firestore client must initialize at startup");

    let listings_state = Arc::new(ListingsState::new(db, rpc_client.clone()));

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

    let shared = Arc::new(SharedState {
        orchestrator: OrchestratorProxy::new(orchestrator_url),
        game_api: GameApiProxy::new(game_api_url)?,
        clawtasks: ClawTasksProxy::new(clawtasks_url),
        botbounty: BotBountyProxy::new(botbounty_url),
        solana_rpc_url,
        rpc_client,
        game_sessions,
        challenge_manager,
        session_binding,
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
