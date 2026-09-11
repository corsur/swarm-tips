//! Standalone deployment of the public MCP module.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    mcp_server::run_standalone().await
}
