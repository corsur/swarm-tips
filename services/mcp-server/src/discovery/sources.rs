//! Pull MCP servers from upstream registries. Phase 1 only consumes the
//! official MCP registry (registry.modelcontextprotocol.io). Future phases
//! will add best-of-mcp-servers and PulseMCP.
//!
//! Each source returns a `Vec<RawServer>` with the source-specific metadata
//! mapped onto the common shape. Errors are returned to the caller so the
//! merge layer can decide how to degrade.

use crate::discovery::models::RawServer;
use anyhow::{Context, Result};

const OFFICIAL_REGISTRY_BASE: &str = "https://registry.modelcontextprotocol.io/v0/servers";

/// Maximum servers to pull from the official registry per cycle. The registry
/// has ~2,000 servers; we cap at 5,000 to give headroom while still bounding
/// the work in case of an upstream growth spike.
const MAX_SERVERS: usize = 5_000;

/// User-Agent so the registry maintainers can identify us if we ever cause
/// a problem. Per the discovery plan: "we don't want to be the asshole that
/// gets banned from the official MCP registry."
const USER_AGENT: &str = "SwarmTipsDiscovery/0.1 (+https://swarm.tips)";

/// Pull the full server list from the official MCP registry, paginated by cursor.
/// Returns whatever we got — caller decides what to do with partial failures.
pub async fn pull_official_registry(client: &reqwest::Client) -> Result<Vec<RawServer>> {
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages_fetched = 0u32;
    // Hard cap on pages to bound time + memory even if pagination loops.
    // 100 servers/page × 100 pages = 10k servers max.
    const MAX_PAGES: u32 = 100;

    loop {
        if pages_fetched >= MAX_PAGES {
            tracing::warn!(
                source = "official_mcp",
                pages_fetched,
                "hit MAX_PAGES safety cap during registry pull"
            );
            break;
        }
        if all.len() >= MAX_SERVERS {
            tracing::warn!(
                source = "official_mcp",
                count = all.len(),
                "hit MAX_SERVERS safety cap during registry pull"
            );
            break;
        }

        let mut url = format!("{OFFICIAL_REGISTRY_BASE}?limit=100");
        if let Some(c) = &cursor {
            url.push_str("&cursor=");
            url.push_str(&urlencoding::encode(c));
        }

        let resp = client
            .get(&url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;

        if !resp.status().is_success() {
            anyhow::bail!("official MCP registry returned {} for {url}", resp.status());
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .context("parse official registry response")?;

        let servers = body
            .get("servers")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();

        if servers.is_empty() {
            break;
        }

        for s in &servers {
            if let Some(raw) = parse_official_server(s) {
                all.push(raw);
            }
        }

        // Cursor for next page
        cursor = body
            .get("metadata")
            .and_then(|m| m.get("nextCursor").or_else(|| m.get("next_cursor")))
            .and_then(|c| c.as_str())
            .map(String::from);

        pages_fetched = pages_fetched.saturating_add(1);

        if cursor.is_none() {
            break;
        }
    }

    tracing::info!(
        source = "official_mcp",
        count = all.len(),
        pages = pages_fetched,
        "pulled official MCP registry"
    );

    Ok(all)
}

/// Parse one server entry from the official registry response.
/// Schema: `{ "server": { "name", "description", "version", "remotes": [...], ... }, "_meta": {...} }`
pub fn parse_official_server(entry: &serde_json::Value) -> Option<RawServer> {
    // Some responses wrap the server in a "server" object; others are flat.
    // Handle both gracefully.
    let server_obj = entry.get("server").unwrap_or(entry);

    let name = server_obj.get("name").and_then(|v| v.as_str())?.to_string();
    if name.is_empty() {
        return None;
    }

    let title = server_obj
        .get("title")
        .and_then(|v| v.as_str())
        .map(String::from);
    let description = server_obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Pull endpoint + transport from the first remote, if any.
    let (endpoint, transport) = server_obj
        .get("remotes")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .map(|first| {
            let url = first.get("url").and_then(|v| v.as_str()).map(String::from);
            let transport = first.get("type").and_then(|v| v.as_str()).map(String::from);
            (url, transport)
        })
        .unwrap_or((None, None));

    // npm package + github repo are sometimes embedded in the schema's
    // packages or repository fields.
    let github_repo = server_obj
        .get("repository")
        .and_then(|r| r.get("url"))
        .and_then(|u| u.as_str())
        .or_else(|| server_obj.get("repository").and_then(|r| r.as_str()))
        .map(String::from);

    let npm_package = server_obj
        .get("packages")
        .and_then(|p| p.as_array())
        .and_then(|arr| arr.first())
        .and_then(|pkg| pkg.get("name").or_else(|| pkg.get("identifier")))
        .and_then(|n| n.as_str())
        .map(String::from);

    Some(RawServer {
        name,
        title,
        description,
        endpoint,
        transport,
        npm_package,
        github_repo,
        source: "official".to_string(),
        upstream_quality_score: None,
        upstream_visitors_estimate: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_official_server_minimal() {
        let json = serde_json::json!({
            "server": {
                "name": "io.github.example/foo",
                "description": "Example MCP server",
                "version": "0.1.0"
            }
        });
        let parsed = parse_official_server(&json).expect("should parse");
        assert_eq!(parsed.name, "io.github.example/foo");
        assert_eq!(parsed.description.as_deref(), Some("Example MCP server"));
        assert_eq!(parsed.source, "official");
    }

    #[test]
    fn parse_official_server_with_remote() {
        let json = serde_json::json!({
            "server": {
                "name": "io.github.example/bar",
                "description": "Bar server",
                "remotes": [
                    {"type": "streamable-http", "url": "https://example.com/mcp"}
                ]
            }
        });
        let parsed = parse_official_server(&json).expect("should parse");
        assert_eq!(parsed.endpoint.as_deref(), Some("https://example.com/mcp"));
        assert_eq!(parsed.transport.as_deref(), Some("streamable-http"));
    }

    #[test]
    fn parse_official_server_flat_form() {
        // Some responses might inline the server fields directly without
        // wrapping in {"server": ...}. The parser should handle both.
        let json = serde_json::json!({
            "name": "io.github.flat/baz",
            "description": "Flat schema"
        });
        let parsed = parse_official_server(&json).expect("should parse flat");
        assert_eq!(parsed.name, "io.github.flat/baz");
    }

    #[test]
    fn parse_official_server_drops_empty_name() {
        let json = serde_json::json!({"server": {"name": ""}});
        assert!(parse_official_server(&json).is_none());
    }

    #[test]
    fn parse_official_server_drops_missing_name() {
        let json = serde_json::json!({"server": {"description": "no name"}});
        assert!(parse_official_server(&json).is_none());
    }
}
