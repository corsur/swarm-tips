//! MCP Server — unified tool server for Swarm Tips.
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

mod a2a;
mod capabilities;
mod composite_trust;
mod config;
mod discovery;
mod errors;
mod game_proxy;
mod game_session;
mod inbox;
mod inbox_http;
mod instructions;
pub mod jwt;
mod listings;
#[cfg(test)]
mod matrix_tests;
pub mod openapi;
mod proxy;
mod rate_limit;
mod reputation;
mod reputation_lookup;
mod request_errors;
mod server;
mod session_binding;
mod solana_reads;
mod solana_tx;
mod surfaces;
#[cfg(test)]
mod tool_surface_tests;
mod traffic_stats;
mod web_position;
mod workflows_trigger;
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

/// Parse `INBOX_SEED_WALLETS` (comma-separated wallets of org-owned seed
/// agents: shillbot-worker, grok) into normalized CAIP-10 mailbox addresses.
/// Invalid entries are skipped with a WARN — a typo must not crash the boot,
/// but it must be visible (kill-gate tagging depends on this list).
fn parse_inbox_seed_wallets(raw: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        match inbox::mailbox_address(entry) {
            Ok(caip10) => {
                set.insert(caip10);
            }
            Err(e) => {
                tracing::warn!(entry, error = %e, "INBOX_SEED_WALLETS entry is not a valid wallet — skipped");
            }
        }
    }
    if !set.is_empty() {
        tracing::info!(count = set.len(), "inbox seed wallets loaded");
    }
    set
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

/// Module transports are selected once at startup. Omitted adapters retain
/// standalone HTTP behavior; a selected adapter never falls back after a write.
#[derive(Default)]
pub struct McpDependencies {
    pub game: Option<Arc<dyn api_transport::RequestTransport>>,
    pub shillbot: Option<Arc<dyn api_transport::RequestTransport>>,
    pub realtime: Option<Arc<dyn game_api_client::ws::RealtimeConnector>>,
}

fn game_proxy(
    url: &str,
    dependencies: &McpDependencies,
    internal_jwt_secret: Option<String>,
) -> anyhow::Result<GameApiProxy> {
    let proxy = GameApiProxy::new(url.to_owned())?.with_internal_jwt_secret(internal_jwt_secret);
    Ok(match &dependencies.game {
        Some(transport) => proxy.with_transport(Arc::clone(transport)),
        None => proxy,
    })
}

pub async fn initialize() -> anyhow::Result<McpRuntime> {
    initialize_with_dependencies(McpDependencies::default()).await
}

pub async fn initialize_with_dependencies(
    dependencies: McpDependencies,
) -> anyhow::Result<McpRuntime> {
    let cfg = load_startup_config().await?;
    log_startup(&cfg);
    let ct = tokio_util::sync::CancellationToken::new();

    let rpc_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("reqwest client must build")?;

    let listings_state = Arc::new(
        ListingsState::new(
            open_firestore(&cfg.gcp_project_id).await,
            rpc_client.clone(),
        )
        .with_shillbot(cfg.orchestrator_url.clone(), dependencies.shillbot.clone()),
    );
    // Durable webhook delivery (W4) + search engine stale-while-revalidate (G7):
    // send_message triggers agent-webhook-delivery; search triggers
    // mcp-discovery-refresh when catalog_age_hours >= 24. No project id
    // (local dev) -> None, and triggers degrade to log-and-skip.
    let workflows = if cfg.gcp_project_id.is_empty() {
        None
    } else {
        Some(Arc::new(workflows_trigger::WorkflowsTrigger::new(
            cfg.gcp_project_id.clone(),
            load_env_or("WORKFLOWS_LOCATION", "us-central1"),
        )))
    };

    let discovery_state =
        build_discovery_state(&cfg.gcp_project_id, &rpc_client, workflows.clone()).await;

    let traffic_stats_state = Arc::new(traffic_stats::TrafficStatsState::new(
        rpc_client.clone(),
        cfg.gcp_project_id.clone(),
        load_env_or("UMAMI_BASE_URL", "https://analytics.coordination.game"),
        config::load_optional_secret(&cfg.gcp_project_id, "umami-share-tokens").await,
    ));

    let game_db = Arc::new(open_firestore(&cfg.gcp_project_id).await);
    let database_id = game_db.get_options().database_id.clone();
    let (rpc_url_mainnet, rpc_url_devnet) = load_per_network_rpcs(&cfg).await;
    let mut game_sessions = GameSessionManager::new(
        cfg.game_api_url.clone(),
        cfg.solana_rpc_url.clone(),
        rpc_url_mainnet.clone(),
        rpc_url_devnet.clone(),
        Arc::clone(&game_db),
    );
    if let Some(transport) = &dependencies.game {
        game_sessions = game_sessions.with_transport(Arc::clone(transport));
    }
    if let Some(connector) = &dependencies.realtime {
        game_sessions = game_sessions.with_realtime(Arc::clone(connector));
    }
    let game_sessions = Arc::new(game_sessions.with_shutdown(ct.child_token()));
    let session_binding = Arc::new(McpSessionBinding::new(Arc::clone(&game_db)));
    // This service's public base URL, handed to the delivery workflow for
    // its POST /internal/webhooks/delivery-result callback. Empty → the
    // workflow skips callbacks (failure counters simply don't advance).
    let self_url = load_env_or("MCP_SELF_URL", "");
    // Support-responder trigger: a message to the support mailbox pings the
    // bridge (SUPPORT_RESPONDER_URL, e.g. its run.app /webhook/inbox) with an
    // HMAC-signed POST keyed by the shared inbox-responder-secret. Empty url or
    // missing secret → disabled (log-and-skip); the message still delivers.
    let responder_url = load_env_or("SUPPORT_RESPONDER_URL", "");
    let responder_secret =
        config::load_optional_secret(&cfg.gcp_project_id, "inbox-responder-secret").await;
    let inbox_state = Arc::new(inbox::Inbox::new(
        game_db,
        workflows,
        self_url,
        responder_url,
        responder_secret,
    ));
    let inbox_seed_wallets = parse_inbox_seed_wallets(&load_env_or("INBOX_SEED_WALLETS", ""));
    // Default false: prod hides the 19 testnet-gated tools from tools/list
    // (they stay callable). Set true for local dev / testnet work.
    let show_testnet_tools = matches!(
        load_env_or("SHOW_TESTNET_TOOLS", "false")
            .to_lowercase()
            .as_str(),
        "true" | "1"
    );

    // game-api's session secret: lets /internal/* calls carry a short-lived
    // token for the MCP-verified wallet (required once game-api enforces
    // INTERNAL_AUTH_REQUIRED). Optional so local/standalone runs still start.
    let game_jwt_secret = config::load_optional_secret(&cfg.gcp_project_id, "jwt-secret").await;
    if game_jwt_secret.is_none() {
        tracing::warn!("jwt-secret unavailable — game-api /internal/* calls go unauthenticated");
    }

    // Browser REST twins for the agent inbox (/internal/inbox/*): the SAME
    // session-binding + inbox handles the MCP tools use, with its own
    // game-api adapter for the auth-challenge passthrough.
    let inbox_http_state = Arc::new(inbox_http::InboxHttpState {
        game_api: game_proxy(&cfg.game_api_url, &dependencies, game_jwt_secret.clone())?,
        session_binding: Arc::clone(&session_binding),
        inbox: Arc::clone(&inbox_state),
        inbox_seed_wallets: inbox_seed_wallets.clone(),
        jwt_secret: game_jwt_secret.clone(),
    });

    let mut orchestrator =
        OrchestratorProxy::new(cfg.orchestrator_url.clone(), cfg.shorts_api_url.clone());
    if let Some(transport) = &dependencies.shillbot {
        orchestrator = orchestrator.with_transport(Arc::clone(transport));
    }
    let oracle_keypair = load_oracle_keypair(&cfg.gcp_project_id).await;
    let shared = Arc::new(SharedState {
        orchestrator: orchestrator.clone(),
        game_api: game_proxy(&cfg.game_api_url, &dependencies, game_jwt_secret.clone())?,
        solana_rpc_url: cfg.solana_rpc_url.clone(),
        solana_rpc_url_mainnet: rpc_url_mainnet.clone(),
        solana_rpc_url_devnet: rpc_url_devnet.clone(),
        rpc_client: rpc_client.clone(),
        game_sessions,
        session_binding,
        listings: Arc::clone(&listings_state),
        discovery: discovery_state.clone(),
        inbox: inbox_state,
        inbox_seed_wallets,
        show_testnet_tools,
        oracle_keypair: oracle_keypair.clone(),
    });

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
    let services = McpServices {
        swarm: streamable_service(Arc::clone(&shared), surfaces::Surface::Swarm, &ct),
        shillbot: streamable_service(Arc::clone(&shared), surfaces::Surface::Shillbot, &ct),
        game: streamable_service(shared, surfaces::Surface::Game, &ct),
    };

    let reputation_db = Arc::new(open_firestore(&cfg.gcp_project_id).await);
    // Shared internal key: /internal/reputation/rebuild is a service-to-service
    // webhook from shillbot-api's settlement-finalize path. Gate it on the same
    // shillbot-api-key the caller sends so it isn't a public unauthenticated
    // EigenTrust-recompute trigger. None → the route rejects all callers.
    let internal_api_key =
        config::load_optional_secret(&cfg.gcp_project_id, "shillbot-api-key").await;
    let verify_state = BuildVerifyState {
        orchestrator,
        jwt_secret: game_jwt_secret,
        oracle_keypair,
        rpc_client,
        rpc_url_mainnet: rpc_url_mainnet.clone(),
        rpc_url_devnet: rpc_url_devnet.clone(),
    };
    let router = build_router(
        listings_state,
        traffic_stats_state,
        discovery_state,
        reputation_db,
        internal_api_key,
        rpc_url_mainnet,
        rpc_url_devnet,
        verify_state,
        inbox_http_state,
        services,
        ct.clone(),
    );

    Ok(McpRuntime {
        database_id,
        router,
        cancellation: ct,
        bind_addr: format!("{}:{}", cfg.host, cfg.port),
    })
}

/// Independently owned MCP state and transport lifecycle for an embedding host.
pub struct McpRuntime {
    database_id: String,
    router: axum::Router,
    cancellation: tokio_util::sync::CancellationToken,
    bind_addr: String,
}

impl McpRuntime {
    pub fn database_id(&self) -> &str {
        &self.database_id
    }

    pub fn router(&self) -> axum::Router {
        self.router.clone()
    }

    pub fn begin_shutdown(&self) {
        self.cancellation.cancel();
    }
}

pub async fn run_standalone() -> anyhow::Result<()> {
    init_tracing();
    let runtime = initialize().await?;
    let listener = tokio::net::TcpListener::bind(&runtime.bind_addr).await?;
    tracing::info!(service = "mcp-server", addr = %runtime.bind_addr, "MCP server ready");
    axum::serve(listener, runtime.router())
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            runtime.begin_shutdown();
        })
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let terminate = async {
        #[cfg(unix)]
        {
            let mut signal =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler must initialize");
            signal.recv().await;
        }
        #[cfg(not(unix))]
        std::future::pending::<()>().await;
    };
    tokio::select! {
        _ = terminate => {},
        result = tokio::signal::ctrl_c() => { result.expect("SIGINT handler must initialize"); }
    }
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

async fn open_firestore_result(
    project_id: &str,
) -> Result<FirestoreDb, firestore::errors::FirestoreError> {
    let mut options = firestore::FirestoreDbOptions::new(project_id.to_owned());
    if let Ok(database_id) = std::env::var("FIRESTORE_DATABASE_ID") {
        options = options.with_database_id(database_id);
    }
    FirestoreDb::with_options(options).await
}

async fn open_firestore(project_id: &str) -> FirestoreDb {
    open_firestore_result(project_id)
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
    workflows: Option<Arc<workflows_trigger::WorkflowsTrigger>>,
) -> Option<Arc<DiscoveryState>> {
    let discovery_db = match open_firestore_result(gcp_project_id).await {
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
        Arc::new(
            DiscoveryState::new(db, rpc_client.clone(), llm_classifier, github_token)
                .with_workflows(workflows),
        )
    })
}

#[derive(Clone)]
struct BuildVerifyState {
    oracle_keypair: Option<Arc<solana_sdk::signature::Keypair>>,
    orchestrator: OrchestratorProxy,
    jwt_secret: Option<String>,
    rpc_client: reqwest::Client,
    rpc_url_mainnet: String,
    rpc_url_devnet: String,
}

async fn load_oracle_keypair(gcp_project_id: &str) -> Option<Arc<solana_sdk::signature::Keypair>> {
    let raw = match config::load_optional_secret(gcp_project_id, "shillbot-attester-keypair").await
    {
        Some(secret) => Some(secret),
        None => std::env::var("ORACLE_AUTHORITY_KEYPAIR_JSON")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                let path = std::env::var("ORACLE_AUTHORITY_KEYPAIR_PATH")
                    .ok()
                    .filter(|v| !v.trim().is_empty())?;
                std::fs::read_to_string(path).ok()
            }),
    };
    let Some(raw) = raw else {
        tracing::warn!(
            service = "mcp-server",
            "shillbot-attester-keypair not found — falling back to legacy verify_task builder"
        );
        return None;
    };
    match solana_tx::parse_keypair_json(&raw) {
        Ok(kp) => {
            use solana_sdk::signer::Signer;
            tracing::info!(
                service = "mcp-server",
                oracle_authority = %kp.pubkey(),
                "loaded oracle_authority keypair for co-signed verify_task"
            );
            Some(Arc::new(kp))
        }
        Err(err) => {
            tracing::error!(
                service = "mcp-server",
                error = %err,
                "failed to parse oracle_authority keypair"
            );
            None
        }
    }
}

/// Per-network RPC URLs for `/internal/build-verify-tx`: the request body
/// picks one (mainnet by default, devnet when `network: "devnet"` is set);
/// without the right URL the lookup hits the wrong cluster and fails.
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
    verify_state: BuildVerifyState,
    inbox_http_state: Arc<inbox_http::InboxHttpState>,
    mcp_services: McpServices,
    shutdown: tokio_util::sync::CancellationToken,
) -> axum::Router {
    let verify_state_get = verify_state.clone();
    let verify_state_post = verify_state;

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
        .route("/", axum::routing::get(root_page))
        .route("/related-servers", axum::routing::get(related_servers_page))
        .route(
            "/.well-known/openapi.json",
            openapi::well_known_openapi_handler(),
        )
        .route(
            "/.well-known/agent.json",
            openapi::well_known_agent_card_handler(),
        )
        .route(
            "/.well-known/agent-card.json",
            openapi::well_known_agent_card_handler(),
        )
        .route(
            "/.well-known/client.json",
            openapi::well_known_client_manifest_handler(),
        )
        .route(
            "/internal/inbox/list",
            inbox_http::list_handler(Arc::clone(&inbox_http_state)),
        )
        .route(
            "/internal/inbox/open",
            inbox_http::open_handler(Arc::clone(&inbox_http_state)),
        )
        .route(
            "/internal/inbox/ack-ids",
            inbox_http::ack_ids_handler(Arc::clone(&inbox_http_state)),
        )
        // Initialization has finished before the router is exposed. Readiness
        // observes this module's lifecycle, without dependency calls per probe.
        .route(
            "/ready",
            axum::routing::get(build_readiness_handler(shutdown)),
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
        // Browser REST twins for the agent inbox — same storage layer and
        // events as the agent_* MCP tools (see src/inbox_http.rs).
        .route(
            "/internal/inbox/session",
            inbox_http::session_handler(Arc::clone(&inbox_http_state)),
        )
        .route(
            "/internal/inbox/messages",
            inbox_http::messages_handler(Arc::clone(&inbox_http_state)),
        )
        .route(
            "/internal/inbox/ack",
            inbox_http::ack_handler(Arc::clone(&inbox_http_state)),
        )
        .route(
            "/internal/inbox/send",
            inbox_http::send_handler(Arc::clone(&inbox_http_state)),
        )
        .route(
            "/internal/inbox/mute",
            inbox_http::mute_handler(Arc::clone(&inbox_http_state)),
        )
        // W4: webhook registration/management + the agent-webhook-delivery
        // workflow's outcome callback.
        .route(
            "/internal/inbox/webhook",
            inbox_http::webhook_handler(Arc::clone(&inbox_http_state)),
        )
        .route(
            "/internal/webhooks/delivery-result",
            inbox_http::delivery_result_handler(Arc::clone(&inbox_http_state)),
        )
        // W3: topic-board twins (publish/report share the inbox session
        // chokepoint; read is open like /internal/listings).
        .route(
            "/internal/topics/publish",
            inbox_http::topics_publish_handler(Arc::clone(&inbox_http_state)),
        )
        .route(
            "/internal/topics/read",
            inbox_http::topics_read_handler(Arc::clone(&inbox_http_state)),
        )
        .route(
            "/internal/topics/report",
            inbox_http::topics_report_handler(Arc::clone(&inbox_http_state)),
        )
        // W5: Google-A2A JSON-RPC facade over the inbox (message/send,
        // tasks/get, tasks/pushNotificationConfig/set). Translation only —
        // same session guard + inbox chokepoint as the twins (src/a2a.rs).
        .route("/a2a", inbox_http::a2a_handler(inbox_http_state))
        .route(
            "/internal/build-verify-tx",
            axum::routing::get(move || {
                let state = verify_state_get.clone();
                async move { build_verify_tx_ready_handler(state).await }
            })
            .post(
                move |headers: axum::http::HeaderMap, body: axum::Json<serde_json::Value>| {
                    let state = verify_state_post.clone();
                    async move { build_verify_tx_handler(headers, body, state).await }
                },
            )
            .options(|| async {
                // CORS preflight for browser and CLI requests
                axum::http::Response::builder()
                    .header("Access-Control-Allow-Origin", "*")
                    .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                    .header(
                        "Access-Control-Allow-Headers",
                        "content-type, authorization, x-api-key",
                    )
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
            "/internal/reputation/record-bounty",
            reputation::record_bounty_handler(reputation_db.clone(), internal_api_key.clone()),
        )
        .route(
            "/internal/reputation/backfill",
            reputation::backfill_handler(
                Arc::clone(&reputation_db),
                reqwest::Client::new(),
                rpc_url_mainnet.clone(),
                internal_api_key.clone(),
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
            reputation_lookup::handler(
                Arc::clone(&reputation_db),
                rpc_url_mainnet.clone(),
                rpc_url_devnet.clone(),
            ),
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
                discovery::refresh_handler(state.clone(), internal_api_key.clone()),
            )
            .route(
                "/internal/mcp/llm-classify",
                discovery::llm_classify_handler(state.clone(), internal_api_key.clone()),
            )
            .route(
                "/internal/mcp/deep-analyze",
                discovery::deep_analyze_handler(state, internal_api_key),
            );
    }

    // Apply body limit to the internal HTTP surface ONLY. The MCP
    // streamable-http transport at /mcp manages its own framing; layering
    // a body limit before nest_service excludes it from the cap.
    let routed_services = mcp_services.clone();
    let http_rate_limiters = rate_limit::HttpRateLimiters::default();
    router
        .layer(axum::extract::DefaultBodyLimit::max(1_048_576))
        .layer(axum::middleware::from_fn_with_state(
            http_rate_limiters,
            rate_limit::enforce_http_rate_limit,
        ))
        .route(
            "/mcp",
            axum::routing::any(move |req: axum::extract::Request| {
                let services = routed_services.clone();
                async move { services.handle(req).await }
            }),
        )
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

type McpTransport = StreamableHttpService<SwarmTipsMcp, LocalSessionManager>;

fn streamable_service(
    shared: Arc<SharedState>,
    surface: surfaces::Surface,
    cancellation: &tokio_util::sync::CancellationToken,
) -> McpTransport {
    StreamableHttpService::new(
        move || Ok(SwarmTipsMcp::new(Arc::clone(&shared), surface)),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_cancellation_token(cancellation.child_token()),
    )
}

#[derive(Clone)]
struct McpServices {
    swarm: McpTransport,
    shillbot: McpTransport,
    game: McpTransport,
}

impl McpServices {
    async fn handle(&self, req: axum::extract::Request) -> axum::response::Response {
        let surface = req
            .headers()
            .get(axum::http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .map(surfaces::Surface::from_host)
            .unwrap_or(surfaces::Surface::Swarm);
        let response = match surface {
            surfaces::Surface::Swarm => self.swarm.handle(req).await,
            surfaces::Surface::Shillbot => self.shillbot.handle(req).await,
            surfaces::Surface::Game => self.game.handle(req).await,
        };
        response.map(axum::body::Body::new)
    }
}

async fn root_page(headers: axum::http::HeaderMap) -> axum::response::Html<String> {
    let surface = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(surfaces::Surface::from_host)
        .unwrap_or(surfaces::Surface::Swarm);
    let (summary, docs) = match surface {
        surfaces::Surface::Swarm => (
            "Free tools and earning capabilities for agents.",
            "https://swarm.tips/docs",
        ),
        surfaces::Surface::Shillbot => (
            "The complete Shillbot earning, client, and paid-video capability surface.",
            "https://shillbot.org",
        ),
        surfaces::Surface::Game => (
            "The complete Coordination Game capability surface.",
            "https://coordination.game",
        ),
    };
    let title = surface.title();
    let related = surface
        .related()
        .map(|server| {
            format!(
                "<li><a href=\"{}\">{}</a> — {}</li>",
                server.mcp_url(),
                server.title(),
                server.description()
            )
        })
        .collect::<Vec<_>>()
        .join("");
    axum::response::Html(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{title}</title></head><body style=\"font-family:system-ui,sans-serif;max-width:42rem;margin:4rem auto;padding:0 1rem;line-height:1.6\"><h1>{title}</h1><p>{summary}</p><p>Connect an MCP client to <code>https://{}/mcp</code> (streamable HTTP).</p><h2>Related servers</h2><ul>{related}</ul><p><a href=\"/related-servers\">Machine-readable related-server directory</a></p><p><a href=\"{docs}\">Product documentation</a></p></body></html>",
        surface.host()
    ))
}

async fn related_servers_page(headers: axum::http::HeaderMap) -> axum::Json<serde_json::Value> {
    let surface = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(surfaces::Surface::from_host)
        .unwrap_or(surfaces::Surface::Swarm);
    axum::Json(surfaces::related_directory(surface))
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
    let client_id = req
        .headers()
        .get(&header)
        .filter(|v| !v.as_bytes().iter().all(|b| b.is_ascii_whitespace()))
        .cloned();
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
/// the ENTIRE tool surface offline to guard the handful of game tools, which
/// surface game-api errors per-call anyway (2026-07-24 outage: stuck NotReady
/// rollout).
/// Cheap module readiness. Operational dependency checks belong to explicit
/// diagnostics and request error telemetry, never routine readiness probes.
fn build_readiness_handler(
    shutdown: tokio_util::sync::CancellationToken,
) -> impl Fn() -> std::future::Ready<(axum::http::StatusCode, &'static str)> + Clone {
    move || {
        std::future::ready(if shutdown.is_cancelled() {
            (axum::http::StatusCode::SERVICE_UNAVAILABLE, "draining")
        } else {
            (axum::http::StatusCode::OK, "ok")
        })
    }
}

const BUILD_VERIFY_CORS: [(&str, &str); 3] = [
    ("Access-Control-Allow-Origin", "*"),
    ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
    (
        "Access-Control-Allow-Headers",
        "content-type, authorization, x-api-key",
    ),
];

/// Readiness probe for `/internal/build-verify-tx` (GET): reports whether the
/// first-party `oracle_authority` keypair is loaded and what its pubkey is.
async fn build_verify_tx_ready_handler(state: BuildVerifyState) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use solana_sdk::signer::Signer;

    match state.oracle_keypair.as_ref() {
        Some(kp) => (
            StatusCode::OK,
            BUILD_VERIFY_CORS,
            axum::Json(serde_json::json!({
                "ready": true,
                "mode": "oracle_authority_cosign",
                "oracle_authority": kp.pubkey().to_string(),
            })),
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            BUILD_VERIFY_CORS,
            axum::Json(serde_json::json!({
                "ready": false,
                "mode": "unconfigured",
                "oracle_authority": serde_json::Value::Null,
            })),
        )
            .into_response(),
    }
}

fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .trim();
    let (scheme, token) = raw.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// HTTP handler for `/internal/build-verify-tx`.
///
/// Requires `Authorization: Bearer <user_jwt>` (`aud: "mcp"`, `typ: "user"`),
/// enforces `jwt.wallet == payer`, forwards the caller's Bearer token to
/// `shillbot-api` (`POST /tasks/{id}/build-verify`), co-signs the `verify_task`
/// transaction with `oracle_authority`, validates the resulting transaction
/// with `shillbot_chain::validate_base64`, and returns
/// `{ transaction, task_pda, transaction_intent }`.
async fn build_verify_tx_handler(
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
    state: BuildVerifyState,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let Some(token) = extract_bearer_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            BUILD_VERIFY_CORS,
            "missing or invalid Authorization: Bearer header",
        )
            .into_response();
    };
    let Some(jwt_secret) = state.jwt_secret.as_deref() else {
        return (
            StatusCode::UNAUTHORIZED,
            BUILD_VERIFY_CORS,
            "JWT verification unavailable",
        )
            .into_response();
    };
    let verified_wallet = match crate::jwt::verify_user_jwt(token, jwt_secret, "mcp") {
        Ok(w) => w,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                BUILD_VERIFY_CORS,
                "invalid or expired user Bearer token",
            )
                .into_response();
        }
    };

    let get_trimmed = |key: &str| {
        body.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string()
    };
    let task_id = get_trimmed("task_id");
    let payer = get_trimmed("payer");
    if task_id.is_empty() || payer.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            BUILD_VERIFY_CORS,
            "missing required fields: task_id and payer",
        )
            .into_response();
    }

    if !crate::jwt::wallets_match(&verified_wallet, &payer) {
        return (
            StatusCode::FORBIDDEN,
            BUILD_VERIFY_CORS,
            "authenticated wallet does not match payer",
        )
            .into_response();
    }

    let opt_task_pda = match body.get("task_pda") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => Some(s.to_string()),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    BUILD_VERIFY_CORS,
                    "task_pda must be a non-empty string when provided",
                )
                    .into_response();
            }
        },
    };
    let opt_score = match body.get("score") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => match v.as_u64() {
            Some(s) => Some(s),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    BUILD_VERIFY_CORS,
                    "score must be an unsigned integer when provided",
                )
                    .into_response();
            }
        },
    };
    let opt_hash = match body.get("hash") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => Some(s.to_string()),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    BUILD_VERIFY_CORS,
                    "hash must be a non-empty hex string when provided",
                )
                    .into_response();
            }
        },
    };

    let (validation_network, rpc_url, net_opt) = match body.get("network") {
        None | Some(serde_json::Value::Null) => ("mainnet", state.rpc_url_mainnet.as_str(), None),
        Some(v) => match v.as_str().map(str::trim) {
            Some("" | "mainnet" | "mainnet-beta") => {
                ("mainnet", state.rpc_url_mainnet.as_str(), None)
            }
            Some("devnet") => ("devnet", state.rpc_url_devnet.as_str(), Some("devnet")),
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    BUILD_VERIFY_CORS,
                    "network must be 'mainnet' or 'devnet'",
                )
                    .into_response();
            }
        },
    };

    let Some(oracle_kp) = state.oracle_keypair.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            BUILD_VERIFY_CORS,
            "oracle signer not configured",
        )
            .into_response();
    };

    let client = state.orchestrator.authenticated(token.to_owned());
    let vdata = match client
        .get_verification_data(&task_id, &payer, net_opt)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                service = "mcp-server",
                task_id = %task_id,
                error = %e,
                "build-verify-tx: failed to fetch authoritative verification data"
            );
            return (
                StatusCode::BAD_GATEWAY,
                BUILD_VERIFY_CORS,
                "failed to validate verification data against shillbot-api",
            )
                .into_response();
        }
    };

    if let Some(ref caller_task_pda) = opt_task_pda {
        if vdata.task_pda != *caller_task_pda {
            return (
                StatusCode::BAD_REQUEST,
                BUILD_VERIFY_CORS,
                "task_pda does not match authoritative task state",
            )
                .into_response();
        }
    }
    if let Some(caller_score) = opt_score {
        if vdata.composite_score != caller_score {
            return (
                StatusCode::BAD_REQUEST,
                BUILD_VERIFY_CORS,
                "score or hash does not match authoritative verification data",
            )
                .into_response();
        }
    }
    if let Some(ref caller_hash) = opt_hash {
        if !vdata.verification_hash.eq_ignore_ascii_case(caller_hash) {
            return (
                StatusCode::BAD_REQUEST,
                BUILD_VERIFY_CORS,
                "score or hash does not match authoritative verification data",
            )
                .into_response();
        }
    }

    let (verified_task_pda, verified_score, verified_hash) = (
        vdata.task_pda,
        vdata.composite_score,
        vdata.verification_hash,
    );

    let tx = match solana_tx::build_co_signed_verify_tx(
        &state.rpc_client,
        rpc_url,
        &payer,
        &verified_task_pda,
        verified_score,
        &verified_hash,
        oracle_kp,
    )
    .await
    {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(
                service = "mcp-server",
                error = %e,
                "co-signed build_verify_tx failed"
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                BUILD_VERIFY_CORS,
                "transaction builder failed",
            )
                .into_response();
        }
    };

    let fee_payer = match shillbot_chain::fee_payer_base64(&tx) {
        Ok(fp) => fp,
        Err(e) => {
            tracing::error!(
                service = "mcp-server",
                error = %e,
                "build-verify-tx: failed to read fee payer from built transaction"
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                BUILD_VERIFY_CORS,
                "transaction validation failed",
            )
                .into_response();
        }
    };
    let sponsor = (fee_payer != payer).then_some(fee_payer);
    let expected_oracle = Some(solana_sdk::signer::Signer::pubkey(oracle_kp.as_ref()).to_string());
    let validation_req = shillbot_chain::ValidationRequest {
        action: shillbot_chain::Action::Verify,
        network: validation_network.to_string(),
        wallet: payer.clone(),
        task_pda: verified_task_pda.clone(),
        escrow_lamports: None,
        content_id: None,
        sponsor,
        expected_accounts: None,
        expected_data: None,
        allowed_payout_to: None,
        require_payout_routing: false,
        switchboard_feed: expected_oracle,
    };
    let transaction_intent = match shillbot_chain::validate_base64(&tx, &validation_req) {
        Ok(intent) => intent,
        Err(e) => {
            tracing::error!(
                service = "mcp-server",
                error = %e,
                "build-verify-tx: built transaction failed local shillbot_chain validation"
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                BUILD_VERIFY_CORS,
                "transaction validation failed",
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        BUILD_VERIFY_CORS,
        axum::Json(serde_json::json!({
            "transaction": tx,
            "task_pda": verified_task_pda,
            "transaction_intent": transaction_intent,
        })),
    )
        .into_response()
}

static BUILDER_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

/// Bound subprocess memory and lifetime. Dropping a cancelled request kills
/// the child; failures are never retried after uncertain completion.
pub(crate) async fn bounded_builder_output(
    mut command: tokio::process::Command,
    deadline: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    let _permit = BUILDER_SLOTS.try_acquire().map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::WouldBlock, "transaction builder busy")
    })?;
    use tokio::io::AsyncReadExt;
    async fn read(mut stream: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        (&mut stream).take(65_537).read_to_end(&mut bytes).await?;
        if bytes.len() > 65_536 {
            return Err(std::io::Error::other(
                "transaction builder output exceeds limit",
            ));
        }
        Ok(bytes)
    }
    let mut child = command
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("stderr unavailable"))?;
    let (status, stdout, stderr) = tokio::time::timeout(deadline, async {
        tokio::try_join!(child.wait(), read(stdout), read(stderr))
    })
    .await
    .map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "transaction builder deadline exceeded",
        )
    })??;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod readiness_tests {
    use solana_sdk::signer::Signer;

    const TEST_JWT_SECRET: &str = "build-verify-test-secret-0123456789abcdef";

    fn future_exp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
            .saturating_add(300)
    }

    fn bearer_headers(token: &str) -> axum::http::HeaderMap {
        let mut h = axum::http::HeaderMap::new();
        h.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        h
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn transaction_builder_output_and_lifetime_are_bounded() {
        use std::time::Duration;
        let mut good = tokio::process::Command::new("sh");
        good.args(["-c", "printf synthetic-transaction"]);
        let output = super::bounded_builder_output(good, Duration::from_secs(2))
            .await
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"synthetic-transaction");
        let mut large = tokio::process::Command::new("sh");
        large.args(["-c", "printf '%070000d' 0"]);
        assert!(super::bounded_builder_output(large, Duration::from_secs(2))
            .await
            .is_err());
        let mut slow = tokio::process::Command::new("sh");
        slow.args(["-c", "exec sleep 30"]);
        let error = super::bounded_builder_output(slow, Duration::from_millis(20))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        // Every caller (HTTP and MCP) acquires inside the shared function.
        // Busy rejection happens before spawning even a nonexistent executable.
        let _held = super::BUILDER_SLOTS.acquire().await.unwrap();
        let error = super::bounded_builder_output(
            tokio::process::Command::new("must-not-be-spawned"),
            Duration::from_secs(2),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    }

    #[tokio::test]
    async fn readiness_follows_module_shutdown_without_network_dependencies() {
        let shutdown = tokio_util::sync::CancellationToken::new();
        let handler = super::build_readiness_handler(shutdown.clone());
        assert_eq!(handler().await.0, axum::http::StatusCode::OK);
        shutdown.cancel();
        assert_eq!(
            handler().await.0,
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn build_verify_tx_handler_rejects_missing_or_svc_auth_with_401() {
        let payer_kp = solana_sdk::signature::Keypair::new();
        let payer = payer_kp.pubkey().to_string();
        let state = super::BuildVerifyState {
            oracle_keypair: Some(std::sync::Arc::new(solana_sdk::signature::Keypair::new())),
            orchestrator: crate::OrchestratorProxy::new(
                "http://127.0.0.1:1".to_string(),
                "http://127.0.0.1:1".to_string(),
            ),
            jwt_secret: Some(TEST_JWT_SECRET.to_string()),
            rpc_client: reqwest::Client::new(),
            rpc_url_mainnet: "http://127.0.0.1:1".to_string(),
            rpc_url_devnet: "http://127.0.0.1:1".to_string(),
        };
        let body = serde_json::json!({ "task_id": "t1", "payer": payer });

        // 1. Missing Authorization header -> 401 + CORS
        let resp = super::build_verify_tx_handler(
            axum::http::HeaderMap::new(),
            axum::Json(body.clone()),
            state.clone(),
        )
        .await;
        assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(
            resp.headers()["access-control-allow-origin"],
            axum::http::HeaderValue::from_static("*")
        );

        // 2. Forged/internal svc token -> 401 (C7 separation)
        let verified = crate::session_binding::VerifiedWallet::new_for_test(&payer);
        let svc_token = crate::jwt::mint_svc_token(&verified, TEST_JWT_SECRET).unwrap();
        let resp_svc =
            super::build_verify_tx_handler(bearer_headers(&svc_token), axum::Json(body), state)
                .await;
        assert_eq!(resp_svc.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn build_verify_tx_handler_rejects_mismatched_payer_with_403() {
        let alice = solana_sdk::signature::Keypair::new().pubkey().to_string();
        let bob = solana_sdk::signature::Keypair::new().pubkey().to_string();
        let state = super::BuildVerifyState {
            oracle_keypair: Some(std::sync::Arc::new(solana_sdk::signature::Keypair::new())),
            orchestrator: crate::OrchestratorProxy::new(
                "http://127.0.0.1:1".to_string(),
                "http://127.0.0.1:1".to_string(),
            ),
            jwt_secret: Some(TEST_JWT_SECRET.to_string()),
            rpc_client: reqwest::Client::new(),
            rpc_url_mainnet: "http://127.0.0.1:1".to_string(),
            rpc_url_devnet: "http://127.0.0.1:1".to_string(),
        };
        let alice_jwt = crate::jwt::mint_user_jwt_for_test(
            &alice,
            TEST_JWT_SECRET,
            &["shillbot", "game", "mcp"],
            future_exp(),
        );
        let resp = super::build_verify_tx_handler(
            bearer_headers(&alice_jwt),
            axum::Json(serde_json::json!({ "task_id": "t1", "payer": bob })),
            state,
        )
        .await;
        assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);
        assert_eq!(
            resp.headers()["access-control-allow-origin"],
            axum::http::HeaderValue::from_static("*")
        );
    }

    #[tokio::test]
    async fn build_verify_tx_handler_rejects_when_oracle_keypair_unconfigured() {
        let payer = "11111111111111111111111111111111";
        let state = super::BuildVerifyState {
            oracle_keypair: None,
            orchestrator: crate::OrchestratorProxy::new(
                "http://127.0.0.1:1".to_string(),
                "http://127.0.0.1:1".to_string(),
            ),
            jwt_secret: Some(TEST_JWT_SECRET.to_string()),
            rpc_client: reqwest::Client::new(),
            rpc_url_mainnet: "http://127.0.0.1:1".to_string(),
            rpc_url_devnet: "http://127.0.0.1:1".to_string(),
        };
        let user_jwt = crate::jwt::mint_user_jwt_for_test(
            payer,
            TEST_JWT_SECRET,
            &["shillbot", "game", "mcp"],
            future_exp(),
        );
        let body = serde_json::json!({
            "task_id": "t1",
            "payer": payer,
        });
        let resp =
            super::build_verify_tx_handler(bearer_headers(&user_jwt), axum::Json(body), state)
                .await;
        assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn build_verify_tx_handler_one_hop_forwards_caller_bearer_and_returns_intent() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let orchestrator_mock = MockServer::start().await;
        let rpc_mock = MockServer::start().await;

        let payer_kp = solana_sdk::signature::Keypair::new();
        let payer = payer_kp.pubkey().to_string();
        let oracle_kp = std::sync::Arc::new(solana_sdk::signature::Keypair::new());
        let task_pda = solana_sdk::signature::Keypair::new().pubkey().to_string();
        let global_state = solana_sdk::signature::Keypair::new().pubkey().to_string();
        let switchboard_feed = solana_sdk::signature::Keypair::new().pubkey().to_string();
        let verification_hash = "ab".repeat(32);
        let user_jwt = crate::jwt::mint_user_jwt_for_test(
            &payer,
            TEST_JWT_SECRET,
            &["shillbot", "game", "mcp"],
            future_exp(),
        );

        // Orchestrator MUST receive the caller's exact Bearer JWT (no minted internal token).
        Mock::given(method("POST"))
            .and(path("/tasks/task-42/build-verify"))
            .and(header("authorization", format!("Bearer {user_jwt}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-42",
                "task_pda": task_pda,
                "composite_score": 850_000u64,
                "verification_hash": verification_hash,
                "global_state": global_state,
                "switchboard_feed": switchboard_feed,
            })))
            .expect(2)
            .mount(&orchestrator_mock)
            .await;

        // Solana RPC returns a deterministic blockhash.
        let recent_bh = solana_sdk::hash::Hash::new_unique().to_string();
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "context": { "slot": 1 },
                    "value": {
                        "blockhash": recent_bh,
                        "lastValidBlockHeight": 100
                    }
                }
            })))
            .mount(&rpc_mock)
            .await;

        let state = super::BuildVerifyState {
            oracle_keypair: Some(oracle_kp),
            orchestrator: crate::OrchestratorProxy::new(
                orchestrator_mock.uri(),
                orchestrator_mock.uri(),
            ),
            jwt_secret: Some(TEST_JWT_SECRET.to_string()),
            rpc_client: reqwest::Client::new(),
            rpc_url_mainnet: rpc_mock.uri(),
            rpc_url_devnet: rpc_mock.uri(),
        };

        // 1-hop body: omit task_pda, score, and hash!
        let resp = super::build_verify_tx_handler(
            bearer_headers(&user_jwt),
            axum::Json(serde_json::json!({
                "task_id": "task-42",
                "payer": payer,
            })),
            state.clone(),
        )
        .await;
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        assert_eq!(
            resp.headers()["access-control-allow-origin"],
            axum::http::HeaderValue::from_static("*")
        );
        assert_eq!(
            resp.headers()["access-control-allow-headers"],
            axum::http::HeaderValue::from_static("content-type, authorization, x-api-key")
        );
        let bytes = axum::body::to_bytes(resp.into_body(), 65_536)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["transaction"].as_str().is_some_and(|s| !s.is_empty()));
        assert_eq!(json["task_pda"], task_pda);
        assert_eq!(json["transaction_intent"]["action"], "verify");
        assert_eq!(json["transaction_intent"]["fee_payer"], payer);

        // Optional score mismatch -> 400 Bad Request
        let bad_resp = super::build_verify_tx_handler(
            bearer_headers(&user_jwt),
            axum::Json(serde_json::json!({
                "task_id": "task-42",
                "payer": payer,
                "score": 999_999u64,
            })),
            state,
        )
        .await;
        assert_eq!(bad_resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }
}
