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

    let umami_base_url = std::env::var("UMAMI_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let traffic_stats_state = Arc::new(traffic_stats::TrafficStatsState::new(
        rpc_client.clone(),
        cfg.gcp_project_id.clone(),
        umami_base_url,
        config::load_optional_secret(&cfg.gcp_project_id, "umami-share-tokens").await,
    ));

    let game_db = Arc::new(open_firestore(&cfg.gcp_project_id).await);
    let database_id = game_db.get_options().database_id.clone();
    let (rpc_url_mainnet, rpc_url_devnet) = load_per_network_rpcs(&cfg).await;
    // game-api's session secret: lets /internal/* calls (and post-stake queue
    // joins in GameSessionManager) carry a short-lived svc token preserving
    // `is_ai: true` for MCP agents, keys deterministic webhook secret derivation
    // and cross-pod Mcp-Session-Id HMAC verification, enables 24h session JWT
    // minting/auto-refresh, and authenticates stateless Bearer /mcp calls.
    let game_jwt_secret = config::load_optional_secret(&cfg.gcp_project_id, "jwt-secret").await;
    if game_jwt_secret.is_none() {
        tracing::warn!("jwt-secret unavailable — game-api /internal/* calls go unauthenticated, Bearer /mcp auth disabled, and session JWT auto-refresh disabled");
    }

    let mut game_sessions = GameSessionManager::new(
        cfg.game_api_url.clone(),
        cfg.solana_rpc_url.clone(),
        rpc_url_mainnet.clone(),
        rpc_url_devnet.clone(),
        Arc::clone(&game_db),
    )
    .with_internal_jwt_secret(game_jwt_secret.clone());
    if let Some(transport) = &dependencies.game {
        game_sessions = game_sessions.with_transport(Arc::clone(transport));
    }
    if let Some(connector) = &dependencies.realtime {
        game_sessions = game_sessions.with_realtime(Arc::clone(connector));
    }
    let game_sessions = Arc::new(game_sessions.with_shutdown(ct.child_token()));

    let session_binding = Arc::new(
        McpSessionBinding::new(Arc::clone(&game_db)).with_jwt_secret(game_jwt_secret.clone()),
    );
    let inbox_session_binding = Arc::new(
        McpSessionBinding::new_inbox(Arc::clone(&game_db)).with_jwt_secret(game_jwt_secret.clone()),
    );

    // This service's public base URL, handed to the delivery workflow for
    // its POST /internal/webhooks/delivery-result callback. Empty → the
    // workflow skips callbacks (failure counters simply don't advance).
    let self_url = load_env_or("MCP_SELF_URL", "");
    // Support-responder trigger: a message to the support mailbox pings the
    // bridge (SUPPORT_RESPONDER_URL, e.g. its run.app /webhook/inbox) with an
    // HMAC-signed POST keyed by the shared inbox-responder-secret. Empty url or
    // missing secret → disabled (log-and-skip); the message still delivers.
    let responder_url = load_env_or("SUPPORT_RESPONDER_URL", "");
    let board_webhook_url = load_env_or("BOARD_WEBHOOK_URL", "");
    let responder_secret =
        config::load_optional_secret(&cfg.gcp_project_id, "inbox-responder-secret").await;
    let inbox_state = Arc::new(
        inbox::Inbox::new(
            game_db,
            workflows,
            self_url,
            responder_url,
            responder_secret,
        )
        .with_board_webhook_url(board_webhook_url)
        .with_webhook_pepper(game_jwt_secret.clone()),
    );
    let inbox_seed_wallets = parse_inbox_seed_wallets(&load_env_or("INBOX_SEED_WALLETS", ""));
    // Default false: prod hides the 19 testnet-gated tools from tools/list
    // (they stay callable). Set true for local dev / testnet work.
    let show_testnet_tools = matches!(
        load_env_or("SHOW_TESTNET_TOOLS", "false")
            .to_lowercase()
            .as_str(),
        "true" | "1"
    );

    // Browser REST twins for the agent inbox (/internal/inbox/*): isolated
    // `inbox_http_sessions` binding namespace (so X-Inbox-Session tokens cannot
    // cross-hijack Mcp-Session-Id bindings or vice versa) + shared inbox store.
    let inbox_http_state = Arc::new(inbox_http::InboxHttpState {
        game_api: game_proxy(&cfg.game_api_url, &dependencies, game_jwt_secret.clone())?,
        session_binding: inbox_session_binding,
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
    let mcp_session_binding = Arc::clone(&session_binding);
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
        jwt_secret: game_jwt_secret.clone(),
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
        mcp_session_binding,
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
    session_binding: Arc<McpSessionBinding>,
    mcp_services: McpServices,
    shutdown: tokio_util::sync::CancellationToken,
) -> axum::Router {
    let mcp_session_state = Arc::new(McpSessionMiddlewareState {
        key: derive_mcp_session_key(verify_state.jwt_secret.as_deref()),
        jwt_secret: verify_state.jwt_secret.clone(),
        session_binding: Some(session_binding),
    });
    let tasks_redirect_base = public_tasks_redirect_base(verify_state.orchestrator.base_url());
    let verify_state_get = verify_state.clone();
    let verify_state_post = verify_state;

    // KEDA scaling signal: in-flight request count across the whole server
    // (including the /mcp agent transport). mcp-server was a single fixed
    // replica — this + the ScaledObject let it scale under a spike so one
    // expensive call can't block every agent.
    let inflight = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut router =
        attach_public_discovery_routes_with_tasks_base(axum::Router::new(), tasks_redirect_base)
            // Human-facing root. swarm.tips links to this host in its nav, and the
            // MCP protocol endpoint lives at /mcp — without this route a browser
            // visit to the bare domain is an empty 404.
            .route("/", axum::routing::get(root_page))
            .route("/related-servers", axum::routing::get(related_servers_page))
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

    let routed_services = mcp_services.clone();
    let http_rate_limiters = rate_limit::HttpRateLimiters::default();
    router
        .route(
            "/mcp",
            axum::routing::any(move |req: axum::extract::Request| {
                let services = routed_services.clone();
                async move { services.handle(req).await }
            }),
        )
        .layer(axum::extract::DefaultBodyLimit::max(1_048_576))
        // Thread a server-signed Mcp-Session-Id on /mcp and reject unissued or
        // forged Mcp-Session-Id headers with 404 before dispatching tool calls.
        .layer(axum::middleware::from_fn_with_state(
            mcp_session_state,
            ensure_mcp_session_id,
        ))
        // HTTP rate limit wraps EVERYTHING (including /mcp) so public MCP
        // transport calls cannot bypass per-IP rate limiting.
        .layer(axum::middleware::from_fn_with_state(
            http_rate_limiters,
            rate_limit::enforce_http_rate_limit,
        ))
        // In-flight counter wraps EVERYTHING (incl. /mcp) so the scaling
        // metric reflects real agent load, not just the internal API.
        .layer(axum::middleware::from_fn_with_state(
            inflight,
            track_inflight,
        ))
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

const MAX_MCP_REQUEST_BYTES: usize = 1_048_576;

#[derive(Clone)]
struct McpServices {
    swarm: McpTransport,
    shillbot: McpTransport,
    game: McpTransport,
}

impl McpServices {
    async fn handle(&self, req: axum::extract::Request) -> axum::response::Response {
        use axum::response::IntoResponse;

        if *req.method() == axum::http::Method::OPTIONS {
            return mcp_preflight_response(req.headers());
        }
        let req_headers = req.headers().clone();
        let (parts, body) = req.into_parts();
        let bytes = match axum::body::to_bytes(body, MAX_MCP_REQUEST_BYTES).await {
            Ok(b) => b,
            Err(_) => {
                let mut resp = (
                    axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                    axum::Json(serde_json::json!({
                        "error": "payload_too_large",
                        "message": "MCP request body exceeds 1 MiB limit.",
                    })),
                )
                    .into_response();
                attach_mcp_cors_headers(&req_headers, resp.headers_mut());
                return resp;
            }
        };
        let req = axum::extract::Request::from_parts(parts, axum::body::Body::from(bytes));
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
        if response.status() == axum::http::StatusCode::METHOD_NOT_ALLOWED {
            return mcp_method_not_allowed_response(&req_headers);
        }
        let mut resp = response.map(axum::body::Body::new);
        attach_mcp_cors_headers(&req_headers, resp.headers_mut());
        resp
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

/// Derive a domain-separated 256-bit HMAC key for `Mcp-Session-Id` tokens so
/// all Cloud Run replicas sharing `jwt-secret` accept each other's session IDs
/// while rejecting arbitrary/forged client-chosen IDs.
fn derive_mcp_session_key(jwt_secret: Option<&str>) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    if let Some(secret) = jwt_secret.map(str::trim).filter(|s| !s.is_empty()) {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
        mac.update(b"swarm-mcp-session-id-v1");
        let bytes = mac.finalize().into_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        return out;
    }
    let mut fallback = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut fallback);
    fallback
}

fn sign_mcp_session_uuid(id: &str, key: &[u8; 32]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(b"mcp-session-v1:");
    mac.update(id.as_bytes());
    let tag = hex::encode(mac.finalize().into_bytes());
    format!("{id}.{tag}")
}

/// Mint a signed `{uuid_v4}.{hmac_sha256_hex}` `Mcp-Session-Id` token.
pub(crate) fn mint_signed_mcp_session_id(key: &[u8; 32]) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sign_mcp_session_uuid(&id, key)
}

/// Derive a deterministic RFC 4122 UUID-v4-shaped signed `Mcp-Session-Id` from
/// `(key, canonical_wallet)` so stateless `Authorization: Bearer <jwt>` MCP
/// clients correlate to the same session across pods and calls without tracking
/// `Mcp-Session-Id`.
pub(crate) fn mint_deterministic_bearer_mcp_session_id(
    key: &[u8; 32],
    canonical_wallet: &str,
) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let canonical = crate::jwt::normalize_wallet(canonical_wallet);
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(b"mcp-bearer-uuid-v1:");
    mac.update(canonical.as_bytes());
    let digest = mac.finalize().into_bytes();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let id = uuid::Uuid::from_bytes(bytes).to_string();
    sign_mcp_session_uuid(&id, key)
}

/// Verify a signed `{uuid}.{hmac_sha256_hex}` `Mcp-Session-Id` token in constant time.
pub(crate) fn verify_signed_mcp_session_id(token: &str, key: &[u8; 32]) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let Some((id, tag_hex)) = token.trim().split_once('.') else {
        return false;
    };
    if uuid::Uuid::parse_str(id).is_err() || tag_hex.len() != 64 {
        return false;
    }
    let Ok(tag_bytes) = hex::decode(tag_hex) else {
        return false;
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(b"mcp-session-v1:");
    mac.update(id.as_bytes());
    mac.verify_slice(&tag_bytes).is_ok()
}

/// Internal header set by `combined-api` ONLY after verifying a Google OIDC
/// callback token on `/_internal/*` (and stripped from all public requests).
pub(crate) const COMBINED_CALLBACK_VERIFIED_HEADER: &str = "x-combined-callback-verified";

fn constant_time_eq_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub(crate) fn bearer_authorized(
    headers: &axum::http::HeaderMap,
    expected_key: Option<&str>,
) -> bool {
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));
    matches!(
        (expected_key, provided),
        (Some(exp), Some(got)) if !exp.is_empty() && constant_time_eq_bytes(got.as_bytes(), exp.as_bytes())
    )
}

/// Shared authorization gate for internal admin and Google Cloud Workflows /
/// Scheduler callback routes:
/// 1. Trusts `x-combined-callback-verified: 1` ONLY when dispatched in-process
///    by `combined-api` (`api_transport::current_caller().is_some()`), so an
///    external HTTP caller can never spoof the header (`CRIT-15`).
/// 2. Otherwise requires `Authorization: Bearer <internal_api_key>` (constant-time,
///    fail-closed when unset or empty).
pub(crate) fn internal_admin_authorized(
    headers: &axum::http::HeaderMap,
    expected_key: Option<&str>,
) -> bool {
    if api_transport::current_caller().is_some()
        && headers
            .get(COMBINED_CALLBACK_VERIFIED_HEADER)
            .and_then(|v| v.to_str().ok())
            == Some("1")
    {
        return true;
    }
    bearer_authorized(headers, expected_key)
}

const FIRST_PARTY_MCP_DOMAINS: &[&str] = &["swarm.tips", "shillbot.org", "coordination.game"];

fn is_valid_loopback_port(port_str: &str) -> bool {
    !port_str.is_empty()
        && port_str.len() <= 5
        && port_str.bytes().all(|b| b.is_ascii_digit())
        && port_str.parse::<u16>().is_ok_and(|p| p > 0)
}

fn is_valid_dns_hostname(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

fn matches_domain_or_subdomain(host_lower: &str, domain: &str) -> bool {
    if !is_valid_dns_hostname(host_lower) {
        return false;
    }
    if host_lower == domain {
        return true;
    }
    host_lower
        .strip_suffix(domain)
        .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
}

fn is_allowed_first_party_https_origin(origin: &str) -> bool {
    if origin.ends_with('/') {
        return false;
    }
    let Ok(parsed) = url::Url::parse(origin) else {
        return false;
    };
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
        || !matches!(parsed.port(), None | Some(443))
    {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host_lower = host.to_ascii_lowercase();
    FIRST_PARTY_MCP_DOMAINS
        .iter()
        .any(|d| matches_domain_or_subdomain(&host_lower, d))
}

fn is_allowed_loopback_origin(origin: &str) -> bool {
    for host in [
        "http://localhost",
        "http://127.0.0.1",
        "http://[::1]",
        "https://localhost",
        "https://127.0.0.1",
        "https://[::1]",
    ] {
        if origin == host {
            return true;
        }
        if let Some(rest) = origin.strip_prefix(host) {
            if let Some(port) = rest.strip_prefix(':') {
                if is_valid_loopback_port(port) {
                    return true;
                }
            }
        }
    }
    false
}

/// Validate a `/mcp` `Host` header against DNS-rebinding attacks (`RUSTSEC-2026-0189`).
/// Accepts first-party domains (`*.swarm.tips`, `*.shillbot.org`, `*.coordination.game`),
/// loopback (`localhost`, `127.0.0.1`, `[::1]`), and Cloud Run / in-cluster hosts.
pub(crate) fn is_allowed_mcp_host(raw_host: &str) -> bool {
    let trimmed = raw_host.trim();
    if trimmed.is_empty()
        || trimmed
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace() || b == b'@' || b == b'/')
    {
        return false;
    }
    let host_only = if let Some(rest) = trimmed.strip_prefix('[') {
        let Some((ipv6, tail)) = rest.split_once(']') else {
            return false;
        };
        if !tail.is_empty() {
            let Some(port) = tail.strip_prefix(':') else {
                return false;
            };
            if !is_valid_loopback_port(port) {
                return false;
            }
        }
        if ipv6 != "::1" {
            return false;
        }
        "[::1]"
    } else if let Some((h, port)) = trimmed.rsplit_once(':') {
        if !is_valid_loopback_port(port) {
            return false;
        }
        h
    } else {
        trimmed
    };

    let lower = host_only.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "localhost" | "127.0.0.1" | "[::1]" | "mcp-server" | "combined-api"
    ) {
        return true;
    }
    if FIRST_PARTY_MCP_DOMAINS
        .iter()
        .any(|d| matches_domain_or_subdomain(&lower, d))
    {
        return true;
    }
    matches_domain_or_subdomain(&lower, "run.app")
        || matches_domain_or_subdomain(&lower, "svc.cluster.local")
        || matches_domain_or_subdomain(&lower, "internal")
}

/// Validate a browser `Origin` header against the strict first-party + loopback
/// allowlist (`P1-6`, `RUSTSEC-2026-0189`). Never returns `*` or reflects untrusted origins.
pub(crate) fn allowed_browser_origin(
    headers: &axum::http::HeaderMap,
) -> Option<axum::http::HeaderValue> {
    let raw = headers
        .get(axum::http::header::ORIGIN)?
        .to_str()
        .ok()?
        .trim();
    if is_allowed_first_party_https_origin(raw) || is_allowed_loopback_origin(raw) {
        axum::http::HeaderValue::from_str(raw).ok()
    } else {
        None
    }
}

/// Attach strict allowlisted CORS headers onto a `/mcp` response (`P1-6`).
/// Always sets `Vary: Origin`; emits `Access-Control-Allow-Origin` and
/// `Access-Control-Expose-Headers: mcp-session-id` ONLY for allowlisted origins.
pub(crate) fn attach_mcp_cors_headers(
    req_headers: &axum::http::HeaderMap,
    resp_headers: &mut axum::http::HeaderMap,
) {
    resp_headers.insert(
        axum::http::header::VARY,
        axum::http::HeaderValue::from_static("Origin"),
    );
    if let Some(origin_val) = allowed_browser_origin(req_headers) {
        resp_headers.insert(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, origin_val);
        resp_headers.insert(
            axum::http::header::ACCESS_CONTROL_ALLOW_METHODS,
            axum::http::HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
        );
        resp_headers.insert(
            axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS,
            axum::http::HeaderValue::from_static(
                "authorization, content-type, accept, mcp-session-id, mcp-protocol-version, last-event-id",
            ),
        );
        resp_headers.insert(
            axum::http::header::ACCESS_CONTROL_EXPOSE_HEADERS,
            axum::http::HeaderValue::from_static("mcp-session-id"),
        );
    }
}

pub(crate) fn mcp_preflight_response(
    req_headers: &axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let mut resp = axum::http::StatusCode::NO_CONTENT.into_response();
    attach_mcp_cors_headers(req_headers, resp.headers_mut());
    resp
}

/// Helpful 405 response for non-POST calls to `/mcp` (e.g. external monitors
/// issuing `GET /mcp` without an SSE session on our stateless Streamable HTTP
/// transport). Advertises `Allow: POST, OPTIONS` and points callers to
/// `POST /mcp` and `/.well-known/agent-card.json`.
pub(crate) fn mcp_method_not_allowed_response(
    req_headers: &axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let mut resp = (
        axum::http::StatusCode::METHOD_NOT_ALLOWED,
        axum::Json(serde_json::json!({
            "error": "method_not_allowed",
            "message": "This stateless MCP Streamable HTTP endpoint accepts JSON-RPC 2.0 requests via POST /mcp.",
            "mcp_endpoint": "POST /mcp",
            "agent_card": "/.well-known/agent-card.json",
            "openapi": "/.well-known/openapi.json",
        })),
    )
        .into_response();
    resp.headers_mut().insert(
        axum::http::header::ALLOW,
        axum::http::HeaderValue::from_static("POST, OPTIONS"),
    );
    attach_mcp_cors_headers(req_headers, resp.headers_mut());
    resp
}

#[derive(Clone)]
pub(crate) struct McpSessionMiddlewareState {
    pub key: [u8; 32],
    pub jwt_secret: Option<String>,
    pub session_binding: Option<Arc<McpSessionBinding>>,
}

/// Ensure every `/mcp` request carries a server-issued, HMAC-signed `Mcp-Session-Id`,
/// and authenticate stateless `Authorization: Bearer <user_jwt>` `/mcp` callers:
/// - Unissued or tampered `Mcp-Session-Id` headers on `/mcp` are rejected with `404 Not Found`.
/// - Malformed, invalid, expired, or `typ: "svc"` `Authorization: Bearer` headers on `/mcp`
///   fail fast with `401 Unauthorized`.
/// - Valid `Authorization: Bearer <user_jwt>` headers derive a deterministic signed
///   `Mcp-Session-Id` (when omitted) and persist the verified wallet + token on the
///   session binding, while rejecting `400 Bad Request` if paired with an `Mcp-Session-Id`
///   already bound to a different wallet.
async fn ensure_mcp_session_id(
    axum::extract::State(state): axum::extract::State<Arc<McpSessionMiddlewareState>>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let header = axum::http::HeaderName::from_static("mcp-session-id");
    let is_mcp = req.uri().path() == "/mcp" || req.uri().path().starts_with("/mcp/");
    if !is_mcp {
        return next.run(req).await;
    }

    if *req.method() == axum::http::Method::OPTIONS {
        return mcp_preflight_response(req.headers());
    }

    let req_headers = req.headers().clone();
    let with_cors = |mut resp: axum::response::Response| {
        attach_mcp_cors_headers(&req_headers, resp.headers_mut());
        resp
    };

    // RUSTSEC-2026-0189 (rmcp 1.3.0) DNS-rebinding defense-in-depth on /mcp:
    // - Absent `Origin` is allowed (non-browser CLI/agent clients).
    // - Any present `Origin` MUST match the strict first-party HTTPS / loopback allowlist.
    // - Any present `Host` MUST match an allowlisted first-party, loopback, or deployment host.
    if req_headers.contains_key(axum::http::header::ORIGIN)
        && allowed_browser_origin(&req_headers).is_none()
    {
        return with_cors(
            (
                axum::http::StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({
                    "error": "forbidden_origin",
                    "message": "Untrusted Origin header rejected on /mcp.",
                })),
            )
                .into_response(),
        );
    }
    if let Some(host_val) = req_headers.get(axum::http::header::HOST) {
        let host_ok = host_val.to_str().ok().is_some_and(is_allowed_mcp_host);
        if !host_ok {
            return with_cors(
                (
                    axum::http::StatusCode::FORBIDDEN,
                    axum::Json(serde_json::json!({
                        "error": "forbidden_host",
                        "message": "Untrusted Host header rejected on /mcp.",
                    })),
                )
                    .into_response(),
            );
        }
    }

    let bearer_auth = if req
        .headers()
        .contains_key(axum::http::header::AUTHORIZATION)
    {
        let Some(token) = extract_bearer_token(req.headers()).map(str::to_owned) else {
            return with_cors(
                (
                    axum::http::StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": "invalid_bearer_token",
                        "message": "Authorization header must use Bearer <jwt>.",
                    })),
                )
                    .into_response(),
            );
        };
        let Some(secret) = state.jwt_secret.as_deref() else {
            return with_cors(
                (
                    axum::http::StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": "bearer_auth_unavailable",
                        "message": "Bearer JWT verification is not configured on this server.",
                    })),
                )
                    .into_response(),
            );
        };
        let Ok(verified) =
            crate::session_binding::VerifiedWallet::from_user_bearer_jwt(&token, secret)
        else {
            return with_cors(
                (
                    axum::http::StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": "invalid_bearer_token",
                        "message": "Invalid, expired, or non-user Bearer token.",
                    })),
                )
                    .into_response(),
            );
        };
        Some((verified, token))
    } else {
        None
    };

    let provided = req
        .headers()
        .get(&header)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    let (session_id, session_value) = match provided {
        Some(existing) => {
            if !verify_signed_mcp_session_id(&existing, &state.key) {
                return with_cors(
                    (
                        axum::http::StatusCode::NOT_FOUND,
                        axum::Json(serde_json::json!({
                            "error": "invalid_mcp_session",
                            "message": "Invalid or unissued Mcp-Session-Id; initialize without Mcp-Session-Id to obtain a server-signed session. Ensure your MCP client echoes the Mcp-Session-Id header returned by initialize, then call register_wallet and agent_verify_wallet.",
                        })),
                    )
                        .into_response(),
                );
            }
            let val = axum::http::HeaderValue::from_str(&existing).ok();
            (existing, val)
        }
        None => {
            let minted = match bearer_auth {
                Some((ref verified, _)) => {
                    mint_deterministic_bearer_mcp_session_id(&state.key, verified.as_str())
                }
                None => mint_signed_mcp_session_id(&state.key),
            };
            let val = axum::http::HeaderValue::from_str(&minted).ok();
            if let Some(ref v) = val {
                req.headers_mut().insert(header.clone(), v.clone());
            }
            (minted, val)
        }
    };

    if let (Some((ref verified, ref token)), Some(ref sb)) = (&bearer_auth, &state.session_binding)
    {
        if let Some(bound_wallet) = sb.resolve(&session_id).await {
            if !crate::jwt::wallets_match(&bound_wallet, verified.as_str()) {
                return with_cors(
                    (
                        axum::http::StatusCode::BAD_REQUEST,
                        axum::Json(serde_json::json!({
                            "error": "conflicting_auth_headers",
                            "message": "Authorization: Bearer wallet conflicts with bound Mcp-Session-Id wallet.",
                        })),
                    )
                        .into_response(),
                );
            }
        }
        let needs_bind = match sb.resolve_access_token(&session_id).await {
            Ok(Some((w, existing_tok))) => {
                !crate::jwt::wallets_match(&w, verified.as_str()) || existing_tok != *token
            }
            _ => true,
        };
        if needs_bind {
            let _ = sb
                .bind_verified_with_token(&session_id, verified, token)
                .await;
        }
    }

    let mut resp = next.run(req).await;
    if !resp.headers().contains_key(&header) {
        if let Some(value) = session_value {
            resp.headers_mut().insert(header, value);
        }
    }
    attach_mcp_cors_headers(&req_headers, resp.headers_mut());
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

const HEALTH_CORS: [(&str, &str); 3] = [
    ("Access-Control-Allow-Origin", "*"),
    ("Access-Control-Allow-Methods", "GET, OPTIONS"),
    ("Access-Control-Max-Age", "3600"),
];

/// Liveness: trivial — proves the process/event-loop is alive without making
/// outbound calls (a slow game-api or Solana RPC must never get a healthy pod
/// killed). Includes browser CORS headers + `204` `OPTIONS` preflight so
/// `https://swarm.tips/health` can probe `mcp.swarm.tips`,
/// `mcp.coordination.game`, and `mcp.shillbot.org` directly from browser JS.
fn health_handler() -> axum::routing::MethodRouter {
    axum::routing::get(|| async { (axum::http::StatusCode::OK, HEALTH_CORS, "ok") })
        .options(|| async { (axum::http::StatusCode::NO_CONTENT, HEALTH_CORS) })
}

const PUBLIC_SHILLBOT_API_URL: &str = "https://api.shillbot.org";

/// Choose the public base URL used for `GET /tasks` 307 redirects.
/// Never leaks in-cluster (`http://shillbot-api:8080`) or internal `*.run.app`
/// hostnames into external `Location` headers.
fn public_tasks_redirect_base(orchestrator_url: &str) -> String {
    if let Ok(url) = std::env::var("PUBLIC_SHILLBOT_API_URL") {
        let trimmed = url.trim().trim_end_matches('/');
        if trimmed.starts_with("https://") {
            return trimmed.to_string();
        }
    }
    let trimmed = orchestrator_url.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") && !trimmed.contains(".run.app") {
        trimmed.to_string()
    } else {
        PUBLIC_SHILLBOT_API_URL.to_string()
    }
}

/// Safely construct `{base_url}/tasks[?{query}]`, rejecting CR/LF, NUL, spaces,
/// or any non-printable ASCII byte in `raw_query` so `Location` cannot be
/// abused for HTTP header injection.
fn build_tasks_redirect_location(base_url: &str, raw_query: Option<&str>) -> Option<String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() || !base.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return None;
    }
    match raw_query {
        None | Some("") => Some(format!("{base}/tasks")),
        Some(q) => {
            if !q.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
                return None;
            }
            Some(format!("{base}/tasks?{q}"))
        }
    }
}

fn tasks_redirect_handler(base_url: String) -> axum::routing::MethodRouter {
    use axum::response::IntoResponse;

    axum::routing::get(move |uri: axum::http::Uri| {
        let base_url = base_url.clone();
        async move {
            let Some(target) = build_tasks_redirect_location(&base_url, uri.query()) else {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    HEALTH_CORS,
                    "invalid query string",
                )
                    .into_response();
            };
            let Ok(location) = axum::http::HeaderValue::from_str(&target) else {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    HEALTH_CORS,
                    "invalid redirect target",
                )
                    .into_response();
            };
            let mut resp =
                (axum::http::StatusCode::TEMPORARY_REDIRECT, HEALTH_CORS).into_response();
            resp.headers_mut()
                .insert(axum::http::header::LOCATION, location);
            resp
        }
    })
    .options(|| async { (axum::http::StatusCode::NO_CONTENT, HEALTH_CORS) })
}

fn robots_txt_handler() -> axum::routing::MethodRouter {
    axum::routing::get(|| async {
        (
            axum::http::StatusCode::OK,
            [
                (
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
                ),
                (
                    axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
                    axum::http::HeaderValue::from_static("*"),
                ),
            ],
            "User-agent: *\nAllow: /\n",
        )
    })
}

#[cfg(test)]
fn attach_public_discovery_routes(router: axum::Router) -> axum::Router {
    attach_public_discovery_routes_with_tasks_base(router, PUBLIC_SHILLBOT_API_URL.to_string())
}

fn attach_public_discovery_routes_with_tasks_base(
    router: axum::Router,
    tasks_redirect_base: String,
) -> axum::Router {
    router
        .route("/health", health_handler())
        .route("/robots.txt", robots_txt_handler())
        .route("/tasks", tasks_redirect_handler(tasks_redirect_base))
        .route(
            "/.well-known/openapi.json",
            openapi::well_known_openapi_handler(),
        )
        .route("/openapi.json", openapi::well_known_openapi_handler())
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

    let opt_attempt_id = match body.get("attempt_id") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => Some(s.to_string()),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    BUILD_VERIFY_CORS,
                    "attempt_id must be a non-empty string when provided",
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
        .get_verification_data_for_attempt(&task_id, &payer, opt_attempt_id.as_deref(), net_opt)
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

    if let Some(evm_call) = vdata.evm_verify_call {
        return (
            StatusCode::OK,
            BUILD_VERIFY_CORS,
            axum::Json(serde_json::json!({
                "task_id": task_id,
                "transaction": "",
                "task_pda": vdata.task_pda,
                "evm_call": evm_call,
            })),
        )
            .into_response();
    }

    let (
        verified_task_pda,
        verified_score,
        verified_hash,
        required_payout_to,
        advance_backer,
        pre_cosigned_tx,
        returned_attempt_id,
        onboarded_advance_tx,
    ) = (
        vdata.task_pda,
        vdata.composite_score,
        vdata.verification_hash,
        vdata.required_payout_to,
        vdata.advance_backer,
        vdata.co_signed_transaction,
        vdata.attempt_id,
        vdata.onboarded_advance_tx,
    );

    let tx = if let Some(attester_tx) = pre_cosigned_tx.filter(|s| !s.trim().is_empty()) {
        attester_tx
    } else {
        match solana_tx::build_co_signed_verify_tx(
            &state.rpc_client,
            rpc_url,
            &payer,
            &verified_task_pda,
            verified_score,
            &verified_hash,
            required_payout_to.as_deref(),
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
    let require_payout_routing = verified_score > 0 && required_payout_to.is_some();
    let allowed_payout_to = (verified_score > 0).then_some(required_payout_to).flatten();
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
        allowed_payout_to: allowed_payout_to.clone(),
        require_payout_routing,
        switchboard_feed: expected_oracle,
        recoup_backer: None,
        recoup_recipient: None,
        expire_task_state: None,
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

    let mut payload = serde_json::json!({
        "task_id": task_id,
        "transaction": tx,
        "task_pda": verified_task_pda,
        "transaction_intent": transaction_intent,
    });
    if let Some(payout_to) = allowed_payout_to {
        payload["payout_to"] = serde_json::Value::String(payout_to);
    }
    if let Some(backer) = advance_backer {
        payload["advance_backer"] = serde_json::Value::String(backer);
    }
    if let Some(aid) = returned_attempt_id {
        payload["attempt_id"] = serde_json::Value::String(aid);
    }
    if let Some(adv_tx) = onboarded_advance_tx {
        payload["onboarded_advance_tx"] = serde_json::Value::String(adv_tx);
    }

    (StatusCode::OK, BUILD_VERIFY_CORS, axum::Json(payload)).into_response()
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
            .expect(3)
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

        // Unknown network is rejected -> 400 Bad Request upfront.
        let bad_net_resp = super::build_verify_tx_handler(
            bearer_headers(&user_jwt),
            axum::Json(serde_json::json!({
                "task_id": "task-42",
                "payer": payer,
                "network": "testnet",
            })),
            state.clone(),
        )
        .await;
        assert_eq!(bad_net_resp.status(), axum::http::StatusCode::BAD_REQUEST);

        // Mainnet oracle-co-signed verify_task (vk=0 social/referral/website/gameplay) succeeds -> 200 OK.
        let mainnet_resp = super::build_verify_tx_handler(
            bearer_headers(&user_jwt),
            axum::Json(serde_json::json!({
                "task_id": "task-42",
                "payer": payer,
                "network": "mainnet",
            })),
            state.clone(),
        )
        .await;
        assert_eq!(mainnet_resp.status(), axum::http::StatusCode::OK);

        // 1-hop devnet body: omit task_pda, score, and hash!
        let resp = super::build_verify_tx_handler(
            bearer_headers(&user_jwt),
            axum::Json(serde_json::json!({
                "task_id": "task-42",
                "payer": payer,
                "network": "devnet",
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
                "network": "devnet",
                "score": 999_999u64,
            })),
            state,
        )
        .await;
        assert_eq!(bad_resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn build_verify_tx_mainnet_vk0_happy_path_asserts_exact_ixs_accounts_signers_and_fee_payer(
    ) {
        use base64::Engine as _;
        use sha2::Digest as _;
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let orchestrator_mock = MockServer::start().await;
        let rpc_mock = MockServer::start().await;

        let caller_kp = solana_sdk::signature::Keypair::new();
        let caller_pk = caller_kp.pubkey();
        let caller = caller_pk.to_string();
        let oracle_kp = std::sync::Arc::new(solana_sdk::signature::Keypair::new());
        let oracle_pk = oracle_kp.pubkey();
        let task_pk = solana_sdk::pubkey::Pubkey::new_unique();
        let task_pda = task_pk.to_string();
        let expected_global_pk = shillbot_chain::global_state_pda();
        let expected_score = 850_000_u64;
        let expected_hash_bytes = [0x5a_u8; 32];
        let verification_hash = hex::encode(expected_hash_bytes);

        let user_jwt = crate::jwt::mint_user_jwt_for_test(
            &caller,
            TEST_JWT_SECRET,
            &["shillbot", "game", "mcp"],
            future_exp(),
        );

        Mock::given(method("POST"))
            .and(path("/tasks/mainnet-vk0-task/build-verify"))
            .and(header("authorization", format!("Bearer {user_jwt}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "mainnet-vk0-task",
                "task_pda": task_pda,
                "composite_score": expected_score,
                "verification_hash": verification_hash,
                "global_state": expected_global_pk.to_string(),
                "switchboard_feed": oracle_pk.to_string(),
            })))
            .expect(1)
            .mount(&orchestrator_mock)
            .await;

        let recent_bh = solana_sdk::hash::Hash::new_unique();
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "context": { "slot": 99 },
                    "value": {
                        "blockhash": recent_bh.to_string(),
                        "lastValidBlockHeight": 999
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
            rpc_url_devnet: "http://127.0.0.1:1".to_string(),
        };

        let resp = super::build_verify_tx_handler(
            bearer_headers(&user_jwt),
            axum::Json(serde_json::json!({
                "task_id": "mainnet-vk0-task",
                "payer": caller,
                "network": "mainnet",
            })),
            state,
        )
        .await;
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let bytes = axum::body::to_bytes(resp.into_body(), 65_536)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["task_pda"], task_pda);
        assert_eq!(json["transaction_intent"]["action"], "verify");
        assert_eq!(json["transaction_intent"]["network"], "mainnet");
        assert_eq!(json["transaction_intent"]["fee_payer"], caller);
        assert_ne!(
            json["transaction_intent"]["fee_payer"],
            oracle_pk.to_string(),
            "oracle authority must NEVER be fee payer"
        );

        let tx_b64 = json["transaction"].as_str().expect("base64 transaction");
        let mut vtx = shillbot_chain::decode_base64(tx_b64).expect("decode VersionedTransaction");

        // 1. Exact signers & fee payer in compiled v0 message header.
        let keys = vtx.message.static_account_keys();
        let header = vtx.message.header();
        assert_eq!(header.num_required_signatures, 2);
        assert_eq!(
            keys[0], caller_pk,
            "account_keys[0] (fee_payer) must be caller"
        );
        assert_eq!(
            keys[1], oracle_pk,
            "account_keys[1] must be co-signing oracle authority"
        );

        // 2. Exact signature slots: caller slot unsigned, oracle partial sig valid over message.
        assert_eq!(vtx.signatures.len(), 2);
        assert_eq!(
            vtx.signatures[0],
            solana_sdk::signature::Signature::default(),
            "caller fee-payer signature slot must remain empty for local signing"
        );
        assert_ne!(
            vtx.signatures[1],
            solana_sdk::signature::Signature::default(),
            "oracle partial signature must be populated"
        );
        let msg_bytes = vtx.message.serialize();
        assert!(
            vtx.signatures[1].verify(oracle_pk.as_ref(), &msg_bytes),
            "oracle partial signature must cryptographically verify over the v0 message"
        );

        // 3. Exact VerifyTask instruction, accounts, writable/signer flags, and serialized args.
        let expected_verify_ix = shillbot_chain::verify_task_co_signed_instruction(
            task_pk,
            oracle_pk,
            expected_score,
            expected_hash_bytes,
        );
        let compute_budget_id: solana_sdk::pubkey::Pubkey =
            "ComputeBudget111111111111111111111111111111"
                .parse()
                .unwrap();
        let program_ixs: Vec<_> = vtx
            .message
            .instructions()
            .iter()
            .filter(|ix| keys[usize::from(ix.program_id_index)] != compute_budget_id)
            .collect();
        assert_eq!(
            program_ixs.len(),
            1,
            "must contain exactly one non-ComputeBudget instruction (VerifyTask)"
        );
        let verify_ix = program_ixs[0];
        assert_eq!(
            keys[usize::from(verify_ix.program_id_index)],
            expected_verify_ix.program_id
        );
        assert_eq!(
            verify_ix.accounts.len(),
            3,
            "VerifyTask takes [task, global_state, oracle_authority]"
        );

        let task_idx = usize::from(verify_ix.accounts[0]);
        let global_idx = usize::from(verify_ix.accounts[1]);
        let oracle_idx = usize::from(verify_ix.accounts[2]);
        assert_eq!(keys[task_idx], task_pk);
        assert!(vtx.message.is_maybe_writable(task_idx, None));
        assert!(!vtx.message.is_signer(task_idx));

        assert_eq!(keys[global_idx], expected_global_pk);
        assert!(!vtx.message.is_maybe_writable(global_idx, None));
        assert!(!vtx.message.is_signer(global_idx));

        assert_eq!(keys[oracle_idx], oracle_pk);
        assert!(!vtx.message.is_maybe_writable(oracle_idx, None));
        assert!(vtx.message.is_signer(oracle_idx));

        let expected_disc = &sha2::Sha256::digest(b"global:verify_task")[..8];
        assert_eq!(verify_ix.data.len(), 48);
        assert_eq!(&verify_ix.data[..8], expected_disc);
        assert_eq!(&verify_ix.data[8..16], &expected_score.to_le_bytes());
        assert_eq!(&verify_ix.data[16..48], &expected_hash_bytes);
        assert_eq!(verify_ix.data, expected_verify_ix.data);

        // 4. Caller adds slot-0 signature -> full signed mainnet tx validates cleanly.
        vtx.signatures[0] = caller_kp.sign_message(&msg_bytes);
        vtx.verify_and_hash_message()
            .expect("both caller and oracle signatures must verify");
        let signed_b64 = base64::engine::general_purpose::STANDARD
            .encode(bincode::serialize(&vtx).expect("serialize signed vtx"));
        let val_req = shillbot_chain::ValidationRequest {
            action: shillbot_chain::Action::Verify,
            network: "mainnet".to_string(),
            wallet: caller,
            task_pda,
            escrow_lamports: None,
            content_id: None,
            sponsor: None,
            expected_accounts: None,
            expected_data: None,
            allowed_payout_to: None,
            require_payout_routing: false,
            switchboard_feed: Some(oracle_pk.to_string()),
            recoup_backer: None,
            recoup_recipient: None,
            expire_task_state: None,
        };
        shillbot_chain::validate_signed_base64(&signed_b64, &val_req)
            .expect("signed mainnet vk=0 verify_task tx must pass shillbot_chain validation");
    }

    #[tokio::test]
    async fn mcp_session_id_middleware_signs_and_rejects_unissued_tokens() {
        let key = super::derive_mcp_session_key(Some(TEST_JWT_SECRET));
        let mw_state = std::sync::Arc::new(super::McpSessionMiddlewareState {
            key,
            jwt_secret: Some(TEST_JWT_SECRET.to_string()),
            session_binding: None,
        });
        let app = axum::Router::new()
            .route(
                "/mcp",
                axum::routing::post(|headers: axum::http::HeaderMap| async move {
                    let sid = headers
                        .get("mcp-session-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    (axum::http::StatusCode::OK, sid)
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                mw_state,
                super::ensure_mcp_session_id,
            ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/mcp");

        // 1. No Mcp-Session-Id -> server mints a signed token and injects it into both req and resp.
        let init_resp = client.post(&url).send().await.unwrap();
        assert_eq!(init_resp.status(), reqwest::StatusCode::OK);
        let minted = init_resp
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(super::verify_signed_mcp_session_id(&minted, &key));
        assert_eq!(init_resp.text().await.unwrap(), minted);

        // 2. Echoing the server-signed Mcp-Session-Id succeeds.
        let ok_resp = client
            .post(&url)
            .header("mcp-session-id", &minted)
            .send()
            .await
            .unwrap();
        assert_eq!(ok_resp.status(), reqwest::StatusCode::OK);
        assert_eq!(ok_resp.text().await.unwrap(), minted);

        // 3. Client-chosen / forged Mcp-Session-Id -> 404 Not Found.
        let forged = client
            .post(&url)
            .header("mcp-session-id", uuid::Uuid::new_v4().to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(forged.status(), reqwest::StatusCode::NOT_FOUND);

        let tampered = format!("{minted}00");
        let tampered_resp = client
            .post(&url)
            .header("mcp-session-id", tampered)
            .send()
            .await
            .unwrap();
        assert_eq!(tampered_resp.status(), reqwest::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn mcp_session_id_middleware_authenticates_stateless_bearer_and_rejects_forged_or_conflicting_tokens(
    ) {
        let key = super::derive_mcp_session_key(Some(TEST_JWT_SECRET));
        let binding = std::sync::Arc::new(
            crate::session_binding::McpSessionBinding::new_in_memory_for_test()
                .with_jwt_secret(Some(TEST_JWT_SECRET.to_string())),
        );
        let mw_state = std::sync::Arc::new(super::McpSessionMiddlewareState {
            key,
            jwt_secret: Some(TEST_JWT_SECRET.to_string()),
            session_binding: Some(std::sync::Arc::clone(&binding)),
        });
        let app = axum::Router::new()
            .route(
                "/mcp",
                axum::routing::post(|headers: axum::http::HeaderMap| async move {
                    let sid = headers
                        .get("mcp-session-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    (axum::http::StatusCode::OK, sid)
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                mw_state,
                super::ensure_mcp_session_id,
            ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/mcp");

        let alice = solana_sdk::signature::Keypair::new().pubkey().to_string();
        let bob = solana_sdk::signature::Keypair::new().pubkey().to_string();
        let alice_jwt = crate::jwt::mint_user_jwt_for_test(
            &alice,
            TEST_JWT_SECRET,
            &["shillbot", "game", "mcp"],
            future_exp(),
        );
        let bob_jwt = crate::jwt::mint_user_jwt_for_test(
            &bob,
            TEST_JWT_SECRET,
            &["shillbot", "game", "mcp"],
            future_exp(),
        );

        // 1. Happy path: stateless POST /mcp with valid user Bearer JWT derives a deterministic
        //    signed Mcp-Session-Id AND auto-persists verified wallet + access_token on McpSessionBinding!
        let r1 = client
            .post(&url)
            .bearer_auth(&alice_jwt)
            .send()
            .await
            .unwrap();
        assert_eq!(r1.status(), reqwest::StatusCode::OK);
        let sid_1 = r1.text().await.unwrap();
        assert!(super::verify_signed_mcp_session_id(&sid_1, &key));
        assert_eq!(
            sid_1,
            super::mint_deterministic_bearer_mcp_session_id(&key, &alice)
        );
        assert_eq!(
            binding
                .resolve_verified(&sid_1)
                .await
                .expect("verified")
                .as_str(),
            alice
        );
        assert_eq!(
            binding
                .resolve_access_token(&sid_1)
                .await
                .unwrap()
                .expect("token"),
            (alice.clone(), alice_jwt.clone())
        );

        // 2. Second stateless call with Alice's Bearer reuses the EXACT same deterministic session ID.
        let r2 = client
            .post(&url)
            .bearer_auth(&alice_jwt)
            .send()
            .await
            .unwrap();
        assert_eq!(r2.status(), reqwest::StatusCode::OK);
        assert_eq!(r2.text().await.unwrap(), sid_1);

        // 3. Invalid / forged / typ=svc Bearer token fails fast with 401 Unauthorized!
        let bad_resp = client
            .post(&url)
            .bearer_auth("forged.jwt.token")
            .send()
            .await
            .unwrap();
        assert_eq!(bad_resp.status(), reqwest::StatusCode::UNAUTHORIZED);

        let svc_jwt = crate::jwt::mint_svc_token(
            &crate::session_binding::VerifiedWallet::new_for_test(&alice),
            TEST_JWT_SECRET,
        )
        .unwrap();
        let svc_resp = client
            .post(&url)
            .bearer_auth(&svc_jwt)
            .send()
            .await
            .unwrap();
        assert_eq!(svc_resp.status(), reqwest::StatusCode::UNAUTHORIZED);

        // 4. Bob's valid Bearer + Alice's bound Mcp-Session-Id rejects with 400 conflicting_auth_headers!
        let conflict_resp = client
            .post(&url)
            .bearer_auth(&bob_jwt)
            .header("mcp-session-id", &sid_1)
            .send()
            .await
            .unwrap();
        assert_eq!(conflict_resp.status(), reqwest::StatusCode::BAD_REQUEST);
        let conflict_json: serde_json::Value = conflict_resp.json().await.unwrap();
        assert_eq!(conflict_json["error"], "conflicting_auth_headers");
        // Alice's session binding MUST remain untouched!
        assert_eq!(
            binding
                .resolve_verified(&sid_1)
                .await
                .expect("still alice")
                .as_str(),
            alice
        );
    }

    #[tokio::test]
    async fn health_and_openapi_routes_include_browser_cors_headers() {
        let app = super::attach_public_discovery_routes(axum::Router::new());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let base = format!("http://{addr}");
        let client = reqwest::Client::new();

        // 1. GET /health -> 200 OK "ok" + Access-Control-Allow-Origin: *
        let health_get = client.get(format!("{base}/health")).send().await.unwrap();
        assert_eq!(health_get.status(), reqwest::StatusCode::OK);
        assert_eq!(
            health_get
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
        assert_eq!(
            health_get
                .headers()
                .get("access-control-allow-methods")
                .and_then(|v| v.to_str().ok()),
            Some("GET, OPTIONS")
        );
        assert_eq!(health_get.text().await.unwrap(), "ok");

        // 2. OPTIONS /health -> 204 No Content + Access-Control-Allow-Origin: *
        let health_opt = client
            .request(reqwest::Method::OPTIONS, format!("{base}/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(health_opt.status(), reqwest::StatusCode::NO_CONTENT);
        assert_eq!(
            health_opt
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
        assert_eq!(
            health_opt
                .headers()
                .get("access-control-allow-methods")
                .and_then(|v| v.to_str().ok()),
            Some("GET, OPTIONS")
        );

        // 3. GET /openapi.json & GET /.well-known/openapi.json -> 200 OK JSON + CORS
        let openapi_alias = client
            .get(format!("{base}/openapi.json"))
            .send()
            .await
            .unwrap();
        assert_eq!(openapi_alias.status(), reqwest::StatusCode::OK);
        assert_eq!(
            openapi_alias
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
        let alias_json: serde_json::Value = openapi_alias.json().await.unwrap();
        assert!(alias_json["openapi"]
            .as_str()
            .is_some_and(|s| s.starts_with("3.1.")));

        let well_known_json: serde_json::Value = client
            .get(format!("{base}/.well-known/openapi.json"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(alias_json, well_known_json);

        let openapi_opt = client
            .request(reqwest::Method::OPTIONS, format!("{base}/openapi.json"))
            .send()
            .await
            .unwrap();
        assert!(openapi_opt.status().is_success());
        assert_eq!(
            openapi_opt
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
    }

    #[tokio::test]
    async fn tasks_redirect_robots_txt_and_mcp_405_hints_prevent_bot_discovery_leaks() {
        let app = super::attach_public_discovery_routes_with_tasks_base(
            axum::Router::new(),
            "https://api.shillbot.org".to_string(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let base = format!("http://{addr}");
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        // 1. GET /tasks?limit=100&network=mainnet -> 307 Temporary Redirect preserving query + CORS
        let tasks_qs = client
            .get(format!("{base}/tasks?limit=100&network=mainnet"))
            .send()
            .await
            .unwrap();
        assert_eq!(tasks_qs.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            tasks_qs
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok()),
            Some("https://api.shillbot.org/tasks?limit=100&network=mainnet")
        );
        assert_eq!(
            tasks_qs
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );

        // 2. Bare GET /tasks -> 307 to https://api.shillbot.org/tasks
        let tasks_bare = client.get(format!("{base}/tasks")).send().await.unwrap();
        assert_eq!(tasks_bare.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            tasks_bare
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok()),
            Some("https://api.shillbot.org/tasks")
        );

        // 3. Query string sanitizer rejects CR/LF / whitespace header injection
        assert_eq!(
            super::build_tasks_redirect_location(
                "https://api.shillbot.org",
                Some("a=1\r\nX-Bad: 1")
            ),
            None
        );
        assert_eq!(
            super::build_tasks_redirect_location("https://api.shillbot.org", Some("a=1 b=2")),
            None
        );
        assert_eq!(
            super::public_tasks_redirect_base("http://shillbot-api:8080"),
            "https://api.shillbot.org"
        );
        assert_eq!(
            super::public_tasks_redirect_base("https://shillbot-api-abc.a.run.app"),
            "https://api.shillbot.org"
        );
        assert_eq!(
            super::public_tasks_redirect_base("https://staging-api.shillbot.org/"),
            "https://staging-api.shillbot.org"
        );

        // 4. GET /robots.txt -> 200 text/plain; charset=utf-8
        let robots = client
            .get(format!("{base}/robots.txt"))
            .send()
            .await
            .unwrap();
        assert_eq!(robots.status(), reqwest::StatusCode::OK);
        assert_eq!(
            robots
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(robots.text().await.unwrap(), "User-agent: *\nAllow: /\n");

        // 5. MCP 405 helper advertises Allow: POST, OPTIONS + JSON pointer to POST /mcp & agent-card.json
        let mut req_headers = axum::http::HeaderMap::new();
        req_headers.insert(
            axum::http::header::ORIGIN,
            axum::http::HeaderValue::from_static("https://swarm.tips"),
        );
        let resp_405 = super::mcp_method_not_allowed_response(&req_headers);
        assert_eq!(
            resp_405.status(),
            axum::http::StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            resp_405
                .headers()
                .get(axum::http::header::ALLOW)
                .and_then(|v| v.to_str().ok()),
            Some("POST, OPTIONS")
        );
        assert_eq!(
            resp_405
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://swarm.tips")
        );
        let body_bytes = axum::body::to_bytes(resp_405.into_body(), 16_384)
            .await
            .unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["mcp_endpoint"], "POST /mcp");
        assert_eq!(body_json["agent_card"], "/.well-known/agent-card.json");
    }

    #[tokio::test]
    async fn build_verify_tx_handler_prepends_set_payout_to_on_outstanding_advance_and_supports_attester_and_evm_passthrough(
    ) {
        use base64::Engine as _;
        use sha2::Digest as _;
        use solana_sdk::signer::Signer as _;
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let orchestrator_mock = MockServer::start().await;
        let rpc_mock = MockServer::start().await;

        let caller_kp = solana_sdk::signature::Keypair::new();
        let caller_pk = caller_kp.pubkey();
        let caller = caller_pk.to_string();
        let backer_pk = solana_sdk::signature::Keypair::new().pubkey();
        let advance_pk = shillbot_chain::advance_pda(&backer_pk, &caller_pk);
        let oracle_kp = std::sync::Arc::new(solana_sdk::signature::Keypair::new());
        let oracle_pk = oracle_kp.pubkey();
        let task_pk = solana_sdk::signature::Keypair::new().pubkey();
        let task_pda = task_pk.to_string();
        let expected_hash_bytes = [0x99u8; 32];
        let expected_hash = hex::encode(expected_hash_bytes);
        let user_jwt = crate::jwt::mint_user_jwt_for_test(
            &caller,
            TEST_JWT_SECRET,
            &["shillbot", "game", "mcp"],
            future_exp(),
        );

        let recent_bh = solana_sdk::hash::Hash::new_unique();
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "context": { "slot": 100 },
                    "value": {
                        "blockhash": recent_bh.to_string(),
                        "lastValidBlockHeight": 1000
                    }
                }
            })))
            .mount(&rpc_mock)
            .await;

        // Case 1: Outstanding advance on vk=0 verify -> [ComputeBudget, ComputeBudget, set_payout_to(advance_pk), VerifyTask]
        Mock::given(method("POST"))
            .and(path("/tasks/task-adv/build-verify"))
            .and(header("authorization", format!("Bearer {user_jwt}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-adv",
                "task_pda": task_pda,
                "composite_score": 850_000_u64,
                "verification_hash": expected_hash,
                "global_state": shillbot_chain::global_state_pda().to_string(),
                "switchboard_feed": oracle_pk.to_string(),
                "required_payout_to": advance_pk.to_string(),
                "advance_backer": backer_pk.to_string(),
            })))
            .expect(1)
            .mount(&orchestrator_mock)
            .await;

        // Case 2: Attester pre-co-signed verify_task_attested (1a) passthrough
        let attested_ix = shillbot_chain::verify_task_attested_instruction(
            task_pk,
            oracle_pk,
            1_000_000,
            expected_hash_bytes,
            None,
        );
        let set_payout_ix =
            shillbot_chain::set_payout_to_instruction(caller_pk, task_pk, advance_pk);
        let msg = solana_sdk::message::v0::Message::try_compile(
            &caller_pk,
            &[set_payout_ix, attested_ix],
            &[],
            recent_bh,
        )
        .unwrap();
        let vmsg = solana_sdk::message::VersionedMessage::V0(msg);
        let mut attester_vtx = solana_sdk::transaction::VersionedTransaction {
            signatures: vec![solana_sdk::signature::Signature::default(); 2],
            message: vmsg,
        };
        let msg_bytes = attester_vtx.message.serialize();
        attester_vtx.signatures[1] = oracle_kp.sign_message(&msg_bytes);
        let attester_tx_b64 = base64::engine::general_purpose::STANDARD
            .encode(bincode::serialize(&attester_vtx).unwrap());

        Mock::given(method("POST"))
            .and(path("/tasks/task-attested/build-verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-attested",
                "task_pda": task_pda,
                "composite_score": 1_000_000_u64,
                "verification_hash": expected_hash,
                "global_state": shillbot_chain::global_state_pda().to_string(),
                "switchboard_feed": oracle_pk.to_string(),
                "required_payout_to": advance_pk.to_string(),
                "advance_backer": backer_pk.to_string(),
                "verification_kind": 1,
                "co_signed_transaction": attester_tx_b64,
            })))
            .expect(1)
            .mount(&orchestrator_mock)
            .await;

        // Case 3: EVM verifyTaskAttested call (1b-EVM) passthrough
        Mock::given(method("POST"))
            .and(path("/tasks/task-evm/build-verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-evm",
                "task_pda": "eip155:84532:0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c:7",
                "composite_score": 1_000_000_u64,
                "verification_hash": expected_hash,
                "global_state": "",
                "switchboard_feed": "",
                "evm_verify_call": {
                    "to": "0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c",
                    "data": "0x3bae608b0000000000000000000000000000000000000000000000000000000000000007",
                    "value_wei": "0",
                    "chain": "eip155:84532"
                }
            })))
            .expect(1)
            .mount(&orchestrator_mock)
            .await;

        let state = super::BuildVerifyState {
            oracle_keypair: Some(std::sync::Arc::clone(&oracle_kp)),
            orchestrator: crate::OrchestratorProxy::new(
                orchestrator_mock.uri(),
                orchestrator_mock.uri(),
            ),
            jwt_secret: Some(TEST_JWT_SECRET.to_string()),
            rpc_client: reqwest::Client::new(),
            rpc_url_mainnet: rpc_mock.uri(),
            rpc_url_devnet: rpc_mock.uri(),
        };

        // Verify Case 1
        let resp1 = super::build_verify_tx_handler(
            bearer_headers(&user_jwt),
            axum::Json(serde_json::json!({
                "task_id": "task-adv",
                "payer": caller,
                "network": "mainnet",
            })),
            state.clone(),
        )
        .await;
        assert_eq!(resp1.status(), axum::http::StatusCode::OK);
        let json1: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp1.into_body(), 65_536)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(json1["payout_to"], advance_pk.to_string());
        assert_eq!(json1["advance_backer"], backer_pk.to_string());
        assert_eq!(json1["transaction_intent"]["fee_payer"], caller);
        assert_ne!(
            json1["transaction_intent"]["fee_payer"],
            oracle_pk.to_string()
        );
        let vtx1 = shillbot_chain::decode_base64(json1["transaction"].as_str().unwrap()).unwrap();
        let keys1 = vtx1.message.static_account_keys();
        assert_eq!(keys1[0], caller_pk);
        assert_eq!(keys1[1], oracle_pk);
        let cb_id: solana_sdk::pubkey::Pubkey = "ComputeBudget111111111111111111111111111111"
            .parse()
            .unwrap();
        let non_cb1: Vec<_> = vtx1
            .message
            .instructions()
            .iter()
            .filter(|ix| keys1[usize::from(ix.program_id_index)] != cb_id)
            .collect();
        assert_eq!(
            non_cb1.len(),
            2,
            "must prepend set_payout_to before VerifyTask"
        );
        assert_eq!(
            &non_cb1[0].data[..8],
            &sha2::Sha256::digest(b"global:set_payout_to")[..8]
        );
        assert_eq!(&non_cb1[0].data[8..40], advance_pk.as_ref());
        assert_eq!(
            &non_cb1[1].data[..8],
            &sha2::Sha256::digest(b"global:verify_task")[..8]
        );

        // Verify Case 2 (1a attester passthrough)
        let resp2 = super::build_verify_tx_handler(
            bearer_headers(&user_jwt),
            axum::Json(serde_json::json!({
                "task_id": "task-attested",
                "payer": caller,
                "network": "mainnet",
            })),
            state.clone(),
        )
        .await;
        assert_eq!(resp2.status(), axum::http::StatusCode::OK);
        let json2: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp2.into_body(), 65_536)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(json2["transaction"], attester_tx_b64);
        assert_eq!(json2["payout_to"], advance_pk.to_string());
        assert_eq!(json2["transaction_intent"]["fee_payer"], caller);

        // Verify Case 3 (1b-EVM passthrough)
        let resp3 = super::build_verify_tx_handler(
            bearer_headers(&user_jwt),
            axum::Json(serde_json::json!({
                "task_id": "task-evm",
                "payer": caller,
                "network": "devnet",
            })),
            state,
        )
        .await;
        assert_eq!(resp3.status(), axum::http::StatusCode::OK);
        let json3: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp3.into_body(), 65_536)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            json3["evm_call"]["to"],
            "0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c"
        );
        assert_eq!(json3["evm_call"]["value_wei"], "0");
    }

    #[tokio::test]
    async fn mcp_cors_preflight_and_post_allow_first_party_origins_and_reject_untrusted_origins() {
        let state = std::sync::Arc::new(super::McpSessionMiddlewareState {
            key: super::derive_mcp_session_key(Some(TEST_JWT_SECRET)),
            jwt_secret: Some(TEST_JWT_SECRET.to_string()),
            session_binding: None,
        });
        let app = axum::Router::new()
            .route(
                "/mcp",
                axum::routing::any(|req: axum::extract::Request| async move {
                    if *req.method() == axum::http::Method::OPTIONS {
                        return super::mcp_preflight_response(req.headers());
                    }
                    use axum::response::IntoResponse;
                    (axum::http::StatusCode::OK, "ok").into_response()
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                state,
                super::ensure_mcp_session_id,
            ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let url = format!("http://{addr}/mcp");
        let client = reqwest::Client::new();

        // 1. Allowlisted first-party origin (`https://swarm.tips`) gets 204 preflight + full MCP CORS headers.
        let preflight = client
            .request(reqwest::Method::OPTIONS, &url)
            .header("Origin", "https://swarm.tips")
            .header("Access-Control-Request-Method", "POST")
            .header(
                "Access-Control-Request-Headers",
                "content-type, mcp-session-id, mcp-protocol-version",
            )
            .send()
            .await
            .unwrap();
        assert_eq!(preflight.status(), reqwest::StatusCode::NO_CONTENT);
        assert_eq!(
            preflight
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://swarm.tips")
        );
        assert_eq!(
            preflight
                .headers()
                .get("access-control-allow-methods")
                .and_then(|v| v.to_str().ok()),
            Some("GET, POST, DELETE, OPTIONS")
        );
        let allow_headers = preflight
            .headers()
            .get("access-control-allow-headers")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(allow_headers.contains("mcp-session-id"));
        assert!(allow_headers.contains("mcp-protocol-version"));
        assert_eq!(
            preflight
                .headers()
                .get("access-control-expose-headers")
                .and_then(|v| v.to_str().ok()),
            Some("mcp-session-id")
        );
        assert_eq!(
            preflight
                .headers()
                .get("vary")
                .and_then(|v| v.to_str().ok()),
            Some("Origin")
        );
        assert!(
            preflight.headers().get("mcp-session-id").is_none(),
            "OPTIONS preflight must not mint a throwaway Mcp-Session-Id"
        );

        // 2. Actual POST /mcp from allowlisted localhost dev origin exposes mcp-session-id.
        let post_ok = client
            .post(&url)
            .header("Origin", "http://localhost:5173")
            .send()
            .await
            .unwrap();
        assert_eq!(post_ok.status(), reqwest::StatusCode::OK);
        assert_eq!(
            post_ok
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("http://localhost:5173")
        );
        assert_eq!(
            post_ok
                .headers()
                .get("access-control-expose-headers")
                .and_then(|v| v.to_str().ok()),
            Some("mcp-session-id")
        );
        assert!(post_ok.headers().contains_key("mcp-session-id"));

        // 3. Untrusted origins (including localhost-prefix subdomain spoofing) NEVER receive Access-Control-Allow-Origin.
        for bad_origin in [
            "https://evil.example",
            "http://localhost.evil.example",
            "https://swarm.tips.evil.example",
        ] {
            let bad_opt = client
                .request(reqwest::Method::OPTIONS, &url)
                .header("Origin", bad_origin)
                .send()
                .await
                .unwrap();
            assert_eq!(bad_opt.status(), reqwest::StatusCode::NO_CONTENT);
            assert!(
                bad_opt
                    .headers()
                    .get("access-control-allow-origin")
                    .is_none(),
                "untrusted origin {bad_origin} must not receive Access-Control-Allow-Origin"
            );

            let bad_post = client
                .post(&url)
                .header("Origin", bad_origin)
                .send()
                .await
                .unwrap();
            assert_eq!(
                bad_post.status(),
                reqwest::StatusCode::FORBIDDEN,
                "POST from untrusted origin {bad_origin} must be rejected with 403"
            );
            assert!(
                bad_post
                    .headers()
                    .get("access-control-allow-origin")
                    .is_none(),
                "POST from untrusted origin {bad_origin} must not receive Access-Control-Allow-Origin"
            );
        }
    }

    #[tokio::test]
    async fn mcp_rejects_dns_rebinding_host_and_origin_while_allowing_non_browser_clients() {
        let state = std::sync::Arc::new(super::McpSessionMiddlewareState {
            key: super::derive_mcp_session_key(Some(TEST_JWT_SECRET)),
            jwt_secret: Some(TEST_JWT_SECRET.to_string()),
            session_binding: None,
        });
        let app = axum::Router::new()
            .route(
                "/mcp",
                axum::routing::any(|| async { (axum::http::StatusCode::OK, "ok") }),
            )
            .layer(axum::middleware::from_fn_with_state(
                state,
                super::ensure_mcp_session_id,
            ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let url = format!("http://{addr}/mcp");
        let client = reqwest::Client::new();

        // 1. Non-browser CLI/agent client (absent Origin) with valid Host succeeds.
        for valid_host in [
            "mcp.swarm.tips",
            "mcp.shillbot.org:443",
            "mcp.coordination.game",
            "localhost:8080",
            "mcp-server-abc.a.run.app",
        ] {
            let resp = client
                .post(&url)
                .header(reqwest::header::HOST, valid_host)
                .send()
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                reqwest::StatusCode::OK,
                "absent Origin + valid Host {valid_host} must succeed"
            );
        }

        // 2. DNS-rebinding Host headers are rejected with 403 Forbidden.
        for bad_host in [
            "evil.example",
            "rebind.attacker.com:8080",
            "swarm.tips.evil.example",
            "localhost.evil.example",
            "not-swarm.tips",
        ] {
            let resp = client
                .post(&url)
                .header(reqwest::header::HOST, bad_host)
                .send()
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                reqwest::StatusCode::FORBIDDEN,
                "DNS-rebinding Host {bad_host} must be rejected with 403"
            );
            let body: serde_json::Value = resp.json().await.unwrap();
            assert_eq!(body["error"], "forbidden_host");
        }

        // 3. Valid subdomain HTTPS Origins are accepted; non-HTTPS or spoofed Origins are rejected.
        let sub_ok = client
            .post(&url)
            .header(reqwest::header::ORIGIN, "https://app.swarm.tips")
            .send()
            .await
            .unwrap();
        assert_eq!(sub_ok.status(), reqwest::StatusCode::OK);

        for bad_origin in [
            "http://mcp.swarm.tips",
            "https://evil.example",
            "https://swarm.tips.evil.example",
            "https://swarm.tips:8443",
            "https://swarm.tips/path",
        ] {
            let resp = client
                .post(&url)
                .header(reqwest::header::ORIGIN, bad_origin)
                .send()
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                reqwest::StatusCode::FORBIDDEN,
                "bad Origin {bad_origin} must be rejected with 403"
            );
            let body: serde_json::Value = resp.json().await.unwrap();
            assert_eq!(body["error"], "forbidden_origin");
        }
    }
}
