//! Pull MCP servers from upstream registries.
//!
//! Active sources:
//!   - Official MCP registry (`registry.modelcontextprotocol.io`)
//!   - `wong2/awesome-mcp-servers` (community markdown awesome-list)
//!   - `appcypher/awesome-mcp-servers` (community markdown awesome-list)
//!   - `tolkonepiu/best-of-mcp-servers` (ranked markdown awesome-list)
//!
//! Future sources gated on credentials / verification: PulseMCP (auth-walled),
//! Smithery (API surface unconfirmed).
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

// -- Awesome-list scrapers (wong2, appcypher) and best-of-mcp scraper --

/// Source labels for the markdown-scraper sources.
pub const SOURCE_AWESOME_WONG2: &str = "awesome-wong2";
pub const SOURCE_AWESOME_APPCYPHER: &str = "awesome-appcypher";
pub const SOURCE_BEST_OF_MCP: &str = "best-of-mcp";

/// Hard cap on entries from any single awesome-list scrape. The biggest list
/// today (wong2) has ~600 entries; 6_000 gives 10x headroom.
const MAX_AWESOME_ENTRIES: usize = 6_000;

/// Hard cap on README size in bytes. Awesome-list READMEs are typically
/// 50-300KB; 5MB is far above any plausible legitimate value.
const MAX_README_BYTES: usize = 5 * 1024 * 1024;

/// Pull and parse a community awesome-list README from a GitHub repo.
/// Used for both `wong2/awesome-mcp-servers` and `appcypher/awesome-mcp-servers`.
pub async fn pull_awesome_mcp(
    client: &reqwest::Client,
    source_label: &'static str,
    owner: &str,
    repo: &str,
) -> Result<Vec<RawServer>> {
    let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/HEAD/README.md");
    let resp = client
        .get(&url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("{source_label} README returned {} for {url}", resp.status());
    }

    let bytes = resp.bytes().await.context("read awesome-list body")?;
    if bytes.len() > MAX_README_BYTES {
        anyhow::bail!(
            "{source_label} README is {} bytes, exceeds {} cap",
            bytes.len(),
            MAX_README_BYTES
        );
    }

    let content = String::from_utf8_lossy(&bytes);
    let parsed = parse_awesome_md(&content, source_label);

    tracing::info!(
        source = source_label,
        count = parsed.len(),
        "scraped awesome-list"
    );
    Ok(parsed)
}

/// Pull and parse the `tolkonepiu/best-of-mcp-servers` ranked README.
/// Best-of uses a `<details><summary>` structure that's distinct from a plain
/// awesome-list, so it has its own parser.
pub async fn pull_best_of_mcp(client: &reqwest::Client) -> Result<Vec<RawServer>> {
    const URL: &str =
        "https://raw.githubusercontent.com/tolkonepiu/best-of-mcp-servers/HEAD/README.md";

    let resp = client
        .get(URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .with_context(|| format!("GET {URL}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("best-of-mcp README returned {}", resp.status());
    }

    let bytes = resp.bytes().await.context("read best-of-mcp body")?;
    if bytes.len() > MAX_README_BYTES {
        anyhow::bail!("best-of-mcp README is {} bytes, exceeds cap", bytes.len());
    }

    let content = String::from_utf8_lossy(&bytes);
    let parsed = parse_best_of_md(&content);

    tracing::info!(
        source = SOURCE_BEST_OF_MCP,
        count = parsed.len(),
        "scraped best-of-mcp"
    );
    Ok(parsed)
}

/// Parse the body of an awesome-list README into RawServer records.
/// Tracks the current `## ` heading as a category hint and prepends it to
/// the description so the Layer 1 classifier sees it.
pub fn parse_awesome_md(content: &str, source_label: &str) -> Vec<RawServer> {
    let mut out: Vec<RawServer> = Vec::new();
    let mut current_section = String::new();
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in content.lines() {
        if out.len() >= MAX_AWESOME_ENTRIES {
            break;
        }

        let trimmed = line.trim_start();

        // Track section headings (## Heading or ### Heading)
        if let Some(rest) = trimmed.strip_prefix("## ") {
            current_section = clean_heading(rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            current_section = clean_heading(rest);
            continue;
        }

        if !trimmed.starts_with("- ") && !trimmed.starts_with("* ") {
            continue;
        }

        let Some((name, url, desc)) = extract_link_and_desc(trimmed) else {
            continue;
        };

        // Filter: must look like an MCP server entry — either points at GitHub
        // or has "mcp" in its description. Drops random links in intro prose.
        let is_github = url.contains("github.com/");
        let mentions_mcp = desc.to_lowercase().contains("mcp");
        if !is_github && !mentions_mcp {
            continue;
        }

        let key = name.to_lowercase();
        if !seen_keys.insert(key) {
            continue;
        }

        let github_repo = if is_github { Some(url.clone()) } else { None };
        let combined_desc = if current_section.is_empty() {
            desc.clone()
        } else {
            format!("[{current_section}] {desc}")
        };

        out.push(RawServer {
            name,
            title: None,
            description: Some(combined_desc),
            endpoint: None,
            transport: None,
            npm_package: None,
            github_repo,
            source: source_label.to_string(),
            upstream_quality_score: None,
            upstream_visitors_estimate: None,
        });
    }

    out
}

/// Parse the body of `tolkonepiu/best-of-mcp-servers/README.md`. Each project
/// is one line with a `<details><summary>` block embedding the GitHub URL,
/// rank notation (`(🥇29 ·  ⭐ 39K)`), description, and language tags.
pub fn parse_best_of_md(content: &str) -> Vec<RawServer> {
    let mut out: Vec<RawServer> = Vec::new();
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in content.lines() {
        if out.len() >= MAX_AWESOME_ENTRIES {
            break;
        }

        if !line.contains("<details><summary>") {
            continue;
        }

        let Some(github_url) = extract_first_github_url(line) else {
            continue;
        };
        let name = repo_path_from_github_url(&github_url);
        if name.is_empty() {
            continue;
        }

        let description = extract_best_of_description(line);
        let key = name.to_lowercase();
        if !seen_keys.insert(key) {
            continue;
        }

        out.push(RawServer {
            name,
            title: None,
            description,
            endpoint: None,
            transport: None,
            npm_package: None,
            github_repo: Some(github_url),
            source: SOURCE_BEST_OF_MCP.to_string(),
            upstream_quality_score: None,
            upstream_visitors_estimate: None,
        });
    }

    out
}

/// Strip emoji prefixes and `<a name="..."></a>` anchors from a markdown
/// heading, leaving just the human-readable category name.
fn clean_heading(raw: &str) -> String {
    let mut s = raw.trim().to_string();

    // Drop leading non-alphanumeric characters (emoji, punctuation) so a
    // heading like "📂 <a name=\"foo\"></a>File Systems" becomes
    // "<a name=\"foo\"></a>File Systems".
    while let Some(first) = s.chars().next() {
        if first.is_ascii_alphanumeric() || first == '<' || first == '_' {
            break;
        }
        let len = first.len_utf8();
        s = s[len..].to_string();
        s = s.trim_start().to_string();
    }

    // Strip an inline `<a name="..."></a>` anchor by jumping past `</a>`.
    if s.starts_with('<') {
        if let Some(end) = s.find("</a>") {
            s = s[end + 4..].trim().to_string();
        }
    }

    s
}

/// Extract `(name, url, description)` from a single markdown list item.
/// Handles the variants seen in awesome-mcp-servers READMEs:
///   - `- **[Name](url)** - desc`
///   - `- <img...> [Name](url) - desc`
///   - `- [Name](url) - desc`
fn extract_link_and_desc(line: &str) -> Option<(String, String, String)> {
    // Find first '[' and the ']( pair that closes it
    let lb = line.find('[')?;
    let after_lb = &line[lb + 1..];
    let rb_rel = after_lb.find("](")?;
    let name_end = lb + 1 + rb_rel;
    let name = line[lb + 1..name_end].trim().to_string();
    if name.is_empty() {
        return None;
    }

    let url_start = name_end + 2;
    let after_url_open = line.get(url_start..)?;
    let rp_rel = after_url_open.find(')')?;
    let url_end = url_start + rp_rel;
    let url = line[url_start..url_end].trim().to_string();
    if url.is_empty() {
        return None;
    }

    // After the closing `)`, the suffix looks like one of:
    //   `** - desc`            (wong2 bold form)
    //   ` - desc`              (appcypher plain form)
    //   `<sup>1</sup> - desc`  (appcypher with disambiguation superscript)
    //   `** - desc with - dashes` (wong2 with dashes inside the description)
    // Strip any closing bold markers and `<sup>...</sup>` tags, then expect
    // a leading "- " separator.
    let mut suffix = line.get(url_end + 1..)?.trim_start();

    // Strip closing bold marker (**) if present.
    if let Some(rest) = suffix.strip_prefix("**") {
        suffix = rest.trim_start();
    }

    // Skip any number of `<sup>...</sup>` superscripts (appcypher uses
    // nested ones like `<sup><sup>1</sup></sup>`).
    while suffix.starts_with("<sup>") {
        let end = suffix.find("</sup>")? + 6;
        suffix = suffix.get(end..)?.trim_start();
    }

    // Now expect a leading dash separator.
    let desc_str = suffix
        .strip_prefix("- ")
        .or_else(|| suffix.strip_prefix("— "))
        .or_else(|| suffix.strip_prefix("– "))?;

    let desc = desc_str.trim().to_string();
    if desc.is_empty() {
        return None;
    }

    Some((name, url, desc))
}

/// Find the first `https://github.com/owner/repo` URL on a line and return it.
/// Stops the URL at the first quote, angle bracket, whitespace, or paren.
fn extract_first_github_url(line: &str) -> Option<String> {
    const KEY: &str = "https://github.com/";
    let start = line.find(KEY)?;
    let rest = &line[start..];
    let end = rest
        .find(|c: char| c == '"' || c == '<' || c == ' ' || c == ')' || c == '\'')
        .unwrap_or(rest.len());
    let url = rest[..end].trim_end_matches('/').to_string();
    if url.len() <= KEY.len() {
        return None;
    }
    Some(url)
}

/// Convert `https://github.com/owner/repo` -> `owner/repo`.
fn repo_path_from_github_url(url: &str) -> String {
    url.strip_prefix("https://github.com/")
        .unwrap_or("")
        .trim_end_matches('/')
        .to_string()
}

/// Extract the project description from a best-of-mcp `<details><summary>` line.
/// Format: `<b><a href="...">name</a></b> (rank · ⭐ stars) - description. <code>...`
/// We anchor on `</a></b>` and take the text between the first ` - ` after that
/// and the next `<code>` or `</summary>`.
fn extract_best_of_description(line: &str) -> Option<String> {
    let anchor_end = line.find("</a></b>")?;
    let after_anchor = &line[anchor_end + 8..];

    let dash_idx = after_anchor.find(" - ")?;
    let after_dash = &after_anchor[dash_idx + 3..];

    let stop_idx = after_dash
        .find("<code>")
        .or_else(|| after_dash.find("</summary>"))
        .unwrap_or(after_dash.len());

    let desc = after_dash[..stop_idx]
        .trim()
        .trim_end_matches('.')
        .trim()
        .to_string();
    if desc.is_empty() {
        return None;
    }
    Some(desc)
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

    // -- awesome-list parser tests --

    #[test]
    fn extract_link_and_desc_wong2_bold_form() {
        let line = "- **[Apify](https://github.com/apify/actors-mcp-server)** - Use 3,000+ pre-built tools to extract data from websites.";
        let (name, url, desc) = extract_link_and_desc(line).expect("should parse");
        assert_eq!(name, "Apify");
        assert_eq!(url, "https://github.com/apify/actors-mcp-server");
        assert!(desc.starts_with("Use 3,000+ pre-built tools"));
    }

    #[test]
    fn extract_link_and_desc_appcypher_img_form() {
        let line = "- <img src=\"https://cdn.simpleicons.org/files/4CAF50\" height=\"14\"/> [Backup](https://github.com/hexitex/MCP-Backup-Server) - Provides file and folder backup and restoration capabilities";
        let (name, url, desc) = extract_link_and_desc(line).expect("should parse");
        assert_eq!(name, "Backup");
        assert_eq!(url, "https://github.com/hexitex/MCP-Backup-Server");
        assert!(desc.contains("backup and restoration"));
    }

    #[test]
    fn extract_link_and_desc_drops_no_dash() {
        // No " - " separator means we can't split off a description.
        let line = "- [Foo](https://example.com)";
        assert!(extract_link_and_desc(line).is_none());
    }

    #[test]
    fn extract_link_and_desc_drops_empty_url() {
        let line = "- [Foo]() - some desc";
        assert!(extract_link_and_desc(line).is_none());
    }

    #[test]
    fn parse_awesome_md_extracts_entries_under_section() {
        let md = r#"
# Some intro

## Browser Automation

- **[21st.dev Magic](https://github.com/21st-dev/magic-mcp)** - Create crafted UI components.
- **[AgentQL](https://github.com/tinyfish-io/agentql-mcp)** - Get structured data from unstructured web.

## Random links

- [Just a blog post](https://example.com/blog) - Random external link with no relation to the protocol.
"#;
        let parsed = parse_awesome_md(md, SOURCE_AWESOME_WONG2);
        assert_eq!(parsed.len(), 2, "should pick up the two MCP entries only");
        assert_eq!(parsed[0].name, "21st.dev Magic");
        assert_eq!(parsed[1].name, "AgentQL");
        // Section heading should be embedded in the description.
        assert!(parsed[0]
            .description
            .as_deref()
            .unwrap()
            .contains("Browser Automation"));
        // Github URL should be captured.
        assert_eq!(
            parsed[0].github_repo.as_deref(),
            Some("https://github.com/21st-dev/magic-mcp")
        );
        // Source label set correctly.
        assert_eq!(parsed[0].source, SOURCE_AWESOME_WONG2);
    }

    #[test]
    fn parse_awesome_md_dedupes_within_one_source() {
        let md = r#"
## Cat A
- **[Foo](https://github.com/owner/foo)** - desc one
## Cat B
- **[Foo](https://github.com/owner/foo)** - desc two
"#;
        let parsed = parse_awesome_md(md, SOURCE_AWESOME_WONG2);
        assert_eq!(parsed.len(), 1, "should dedupe by lowercased name");
    }

    #[test]
    fn parse_awesome_md_keeps_non_github_when_mentions_mcp() {
        let md = r#"
## Hosted
- **[Audioscrape](https://www.audioscrape.com/docs/mcp)** - Official remote MCP server for podcast search.
"#;
        let parsed = parse_awesome_md(md, SOURCE_AWESOME_APPCYPHER);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Audioscrape");
        assert!(parsed[0].github_repo.is_none());
    }

    #[test]
    fn clean_heading_strips_emoji_and_anchors() {
        assert_eq!(
            clean_heading("📂 <a name=\"file-systems\"></a>File Systems"),
            "File Systems"
        );
        assert_eq!(clean_heading("Aggregators"), "Aggregators");
        assert_eq!(clean_heading("🔄 Version Control"), "Version Control");
    }

    // -- best-of-mcp parser tests --

    #[test]
    fn extract_first_github_url_picks_first_repo() {
        let line = r#"<details><summary><b><a href="https://github.com/mindsdb/mindsdb">mindsdb/mindsdb</a></b> (🥇29 ·  ⭐ 39K) - Connect data."#;
        assert_eq!(
            extract_first_github_url(line).as_deref(),
            Some("https://github.com/mindsdb/mindsdb")
        );
    }

    #[test]
    fn extract_first_github_url_returns_none_when_absent() {
        assert!(extract_first_github_url("plain text").is_none());
    }

    #[test]
    fn repo_path_from_github_url_simple() {
        assert_eq!(
            repo_path_from_github_url("https://github.com/foo/bar"),
            "foo/bar"
        );
        assert_eq!(
            repo_path_from_github_url("https://github.com/foo/bar/"),
            "foo/bar"
        );
        assert_eq!(repo_path_from_github_url("https://example.com"), "");
    }

    #[test]
    fn extract_best_of_description_strips_code_tags() {
        let line = r#"<details><summary><b><a href="https://github.com/foo/bar">foo/bar</a></b> (🥇29 ·  ⭐ 39K) - The description here. <code>tag</code></summary>"#;
        assert_eq!(
            extract_best_of_description(line).as_deref(),
            Some("The description here")
        );
    }

    #[test]
    fn extract_best_of_description_handles_summary_close_only() {
        let line = r#"<details><summary><b><a href="https://github.com/foo/bar">foo/bar</a></b> (rank) - Short text.</summary>"#;
        assert_eq!(
            extract_best_of_description(line).as_deref(),
            Some("Short text")
        );
    }

    #[test]
    fn parse_best_of_md_extracts_entries() {
        let md = r#"
# Header

<br>

## Aggregators

<details><summary><b><a href="https://github.com/mindsdb/mindsdb">mindsdb/mindsdb</a></b> (🥇29 ·  ⭐ 39K) - Connect and unify data across various platforms with MindsDB. <code>❗Unlicensed</code></summary>

- [GitHub](https://github.com/mindsdb/mindsdb) (👨‍💻 890 · 🔀 6.2K)
</details>
<details><summary><b><a href="https://github.com/PipedreamHQ/pipedream">PipedreamHQ/pipedream</a></b> (🥇29 ·  ⭐ 11K) - Connect with 2,500 APIs and prebuilt tools. <code>MIT</code></summary>
</details>
"#;
        let parsed = parse_best_of_md(md);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "mindsdb/mindsdb");
        assert_eq!(
            parsed[0].github_repo.as_deref(),
            Some("https://github.com/mindsdb/mindsdb")
        );
        assert!(parsed[0]
            .description
            .as_deref()
            .unwrap()
            .contains("MindsDB"));
        assert_eq!(parsed[0].source, SOURCE_BEST_OF_MCP);
        assert_eq!(parsed[1].name, "PipedreamHQ/pipedream");
    }

    #[test]
    fn parse_best_of_md_dedupes_within_source() {
        let md = r#"
<details><summary><b><a href="https://github.com/foo/bar">foo/bar</a></b> (rank) - first.</summary></details>
<details><summary><b><a href="https://github.com/foo/bar">foo/bar</a></b> (rank) - second.</summary></details>
"#;
        let parsed = parse_best_of_md(md);
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn parse_best_of_md_skips_lines_without_details() {
        let md = "## Just a heading\n\nplain paragraph\n";
        assert!(parse_best_of_md(md).is_empty());
    }
}
