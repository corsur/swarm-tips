#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

#[allow(dead_code)]
mod auth;
mod errors;
mod game_proxy;
#[allow(dead_code)]
mod game_session;
mod proxy;
#[allow(dead_code)]
mod session;
mod tools;

use async_trait::async_trait;
use rust_mcp_sdk::event_store::InMemoryEventStore;
use rust_mcp_sdk::mcp_server::{
    hyper_server, HyperServerOptions, ServerHandler, ToMcpServerHandler,
};
use rust_mcp_sdk::schema::{
    schema_utils::CallToolError, CallToolRequestParams, CallToolResult, Implementation,
    InitializeResult, ListToolsResult, PaginatedRequestParams, ProtocolVersion, RpcError,
    ServerCapabilities, ServerCapabilitiesTools,
};
use rust_mcp_sdk::McpServer;
use std::sync::Arc;

use crate::auth::ChallengeManager;
use crate::game_proxy::GameApiProxy;
use crate::game_session::GameSessionManager;
use crate::proxy::OrchestratorProxy;
use crate::session::SessionManager;
use crate::tools::{CoordinationTools, ToolState};

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_HOST: &str = "0.0.0.0";

struct CoordinationHandler {
    state: Arc<ToolState>,
    #[allow(dead_code)]
    challenge_manager: Arc<ChallengeManager>,
}

#[async_trait]
impl ServerHandler for CoordinationHandler {
    async fn handle_list_tools_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: CoordinationTools::tools(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<CallToolResult, CallToolError> {
        let tool: CoordinationTools =
            CoordinationTools::try_from(params).map_err(CallToolError::new)?;

        // In production, the wallet_pubkey is extracted from the authenticated
        // MCP session (via the session ID header or auth token). For now, we use
        // a placeholder that will be wired up when the auth middleware is complete.
        let wallet_pubkey = "unauthenticated";

        tools::execute_tool(tool, &self.state, wallet_pubkey).await
    }
}

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
    let host = load_env_or("HOST", DEFAULT_HOST);
    let port: u16 = load_env_or("PORT", &DEFAULT_PORT.to_string())
        .parse()
        .expect("PORT must be a valid u16");

    tracing::info!(
        service = "coordination-mcp-server",
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
        .expect("reqwest client must build at startup");

    let game_sessions = Arc::new(GameSessionManager::new(
        game_api_url.clone(),
        solana_rpc_url.clone(),
    ));

    let tool_state = Arc::new(ToolState {
        orchestrator: OrchestratorProxy::new(orchestrator_url),
        game_api: GameApiProxy::new(game_api_url),
        sessions: sessions.clone(),
        solana_rpc_url,
        program_id,
        rpc_client,
        game_sessions,
    });

    let handler = CoordinationHandler {
        state: tool_state,
        challenge_manager,
    };

    let server_info = InitializeResult {
        server_info: Implementation {
            name: "coordination-mcp-server".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("Coordination DAO MCP Server".into()),
            description: Some("Unified MCP server for the Coordination DAO ecosystem. Play the Coordination Game (anonymous AI detection) and claim Shillbot content creation tasks.".into()),
            icons: vec![],
            website_url: Some("https://coordination.game".into()),
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        protocol_version: ProtocolVersion::V2025_11_25.into(),
        instructions: Some(
            "Coordination DAO MCP server. Two products: \
             (1) Coordination Game — anonymous 1v1 chat, guess human or AI, stake 0.05 SOL. \
             (2) Shillbot — claim content creation tasks, earn crypto for verified engagement. \
             Authenticate with your Solana wallet."
                .into(),
        ),
        meta: None,
    };

    let server = hyper_server::create_server(
        server_info,
        handler.to_mcp_server_handler(),
        HyperServerOptions {
            host,
            port,
            event_store: Some(Arc::new(InMemoryEventStore::default())),
            health_endpoint: Some("/health".into()),
            ..Default::default()
        },
    );

    tracing::info!(service = "coordination-mcp-server", "MCP server ready");

    server
        .start()
        .await
        .map_err(|e| anyhow::anyhow!("server error: {e}"))?;

    Ok(())
}
