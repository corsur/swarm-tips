#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

#[allow(dead_code)]
mod auth;
mod botbounty_proxy;
mod clawtasks_proxy;
mod errors;
mod game_proxy;
#[allow(dead_code)]
mod game_session;
mod proxy;
mod server;
#[allow(dead_code)]
mod session;
mod solana_tx;

use crate::auth::ChallengeManager;
use crate::botbounty_proxy::BotBountyProxy;
use crate::clawtasks_proxy::ClawTasksProxy;
use crate::game_proxy::GameApiProxy;
use crate::game_session::GameSessionManager;
use crate::proxy::OrchestratorProxy;
use crate::server::{SharedState, SwarmTipsMcp};
use crate::session::SessionManager;
use anyhow::Context;
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
    let solana_rpc_url = load_env_or("SOLANA_RPC_URL", "https://api.devnet.solana.com");
    let program_id = load_env_or("SHILLBOT_PROGRAM_ID", "11111111111111111111111111111111");
    let clawtasks_url = load_env_or("CLAWTASKS_API_URL", "https://clawtasks.com/api");
    let botbounty_url = load_env_or("BOTBOUNTY_API_URL", "https://botbounty-production.up.railway.app/api");
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

    let sessions = SessionManager::new();
    let challenge_manager = ChallengeManager::new();

    let rpc_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("reqwest client must build")?;

    let game_sessions = Arc::new(GameSessionManager::new(
        game_api_url.clone(),
        solana_rpc_url.clone(),
    ));

    let shared = Arc::new(SharedState {
        orchestrator: OrchestratorProxy::new(orchestrator_url),
        game_api: GameApiProxy::new(game_api_url)?,
        clawtasks: ClawTasksProxy::new(clawtasks_url),
        botbounty: BotBountyProxy::new(botbounty_url),
        sessions,
        solana_rpc_url,
        program_id,
        rpc_client,
        game_sessions,
        challenge_manager,
    });

    let ct = tokio_util::sync::CancellationToken::new();

    let service = StreamableHttpService::new(
        move || Ok(SwarmTipsMcp::new(shared.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(ct.child_token()),
    );

    let router = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .nest_service("/mcp", service);
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
