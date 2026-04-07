//! Layer 3 deep analysis — heaviest, most expensive pass. Runs on the top N
//! earning candidates only, never on the whole index. Pulls:
//!
//! - npm weekly downloads (free, no auth)
//! - GitHub repo metadata: stargazers count (auth optional)
//! - GitHub README + regex extraction of Solana program IDs and EVM contract
//!   addresses (the input to a future on-chain volume lookup pass)
//!
//! Deliberately NOT in scope for v1:
//!
//! - Live `tools/list` MCP probe — would need a full initialize handshake,
//!   stateful session id, SSE parsing for some servers. Defer until we have
//!   a clear win from it.
//! - On-chain RPC volume lookups — kept as a TODO; the extracted addresses
//!   above are the input that pass would consume.
//! - Active testing of MCP tools (calling them with sample inputs) — Phase 4
//!   in the original Track 3 plan, way past v1.
//!
//! All HTTP calls are best-effort: any individual lookup failure is logged
//! and skipped, never propagated. The whole point is to get *some* signal
//! on as many candidates as possible without one bad server tanking the run.

use crate::discovery::models::{
    EnrichedServer, ExtractedAddress, Layer3Analysis, MCP_SERVERS_COLLECTION,
};
use anyhow::{Context, Result};
use chrono::Utc;
use firestore::FirestoreDb;
use serde::Deserialize;
use std::sync::Arc;

/// Per-cycle cap on Layer 3 deep analysis. Layer 3 is much more expensive
/// per server than Layer 2 (multiple HTTP calls + GitHub rate limits) so the
/// cap is lower.
pub const MAX_SERVERS_PER_CYCLE: usize = 50;

const USER_AGENT: &str = "SwarmTipsDiscovery/0.1 (+https://swarm.tips)";

/// Run a Layer 3 pass over the top earning candidates, write results back
/// to Firestore. Caller is responsible for picking which servers to analyze
/// — see `select_top_candidates`.
pub async fn run_deep_analysis(
    db: &FirestoreDb,
    http: &reqwest::Client,
    candidates: &[EnrichedServer],
) -> Result<DeepAnalysisSummary> {
    let started = std::time::Instant::now();
    let total = candidates.len();
    let mut probed = 0usize;
    let mut skipped = 0usize;
    let mut writes = 0usize;
    let mut write_errors = 0usize;
    let mut addresses_found = 0usize;

    for server in candidates {
        let analysis = analyze_one(http, server).await;
        if !analysis.probed {
            skipped = skipped.saturating_add(1);
        } else {
            probed = probed.saturating_add(1);
            addresses_found = addresses_found.saturating_add(analysis.extracted_addresses.len());
        }

        let mut updated = server.clone();
        updated.layer3_analysis = Some(analysis);
        let slug = updated.slug();
        match db
            .fluent()
            .update()
            .in_col(MCP_SERVERS_COLLECTION)
            .document_id(&slug)
            .object(&updated)
            .execute::<()>()
            .await
        {
            Ok(_) => writes = writes.saturating_add(1),
            Err(e) => {
                write_errors = write_errors.saturating_add(1);
                tracing::warn!(slug, error = %e, "layer3 write failed");
            }
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    tracing::info!(
        total,
        probed,
        skipped,
        addresses_found,
        writes,
        write_errors,
        elapsed_ms,
        "run_deep_analysis complete"
    );

    Ok(DeepAnalysisSummary {
        considered: total,
        probed,
        skipped,
        addresses_found,
        firestore_writes: writes,
        firestore_write_errors: write_errors,
        elapsed_ms,
    })
}

/// Pick the top N earning candidates to deep-analyze. Sort key: source_count
/// desc (multi-source agreement is the strongest priority signal we have
/// without authenticated metrics), then by name for stable ordering.
pub fn select_top_candidates(servers: Vec<EnrichedServer>, n: usize) -> Vec<EnrichedServer> {
    let mut candidates: Vec<EnrichedServer> = servers
        .into_iter()
        .filter(|s| s.is_earning_candidate())
        .collect();
    candidates.sort_by(|a, b| {
        b.source_count
            .cmp(&a.source_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    candidates.truncate(n);
    candidates
}

/// Analyze one server. Best-effort across npm + GitHub — any individual
/// lookup failure is swallowed and the relevant field is left None.
async fn analyze_one(http: &reqwest::Client, server: &EnrichedServer) -> Layer3Analysis {
    let probed_at = Utc::now();
    let mut analysis = Layer3Analysis {
        extracted_addresses: Vec::new(),
        npm_weekly_downloads: None,
        github_stars: None,
        readme_excerpt: None,
        probed: false,
        probed_at,
    };

    if server.npm_package.is_none() && server.github_repo.is_none() {
        return analysis;
    }
    analysis.probed = true;

    // npm weekly downloads
    if let Some(pkg) = &server.npm_package {
        if let Some(dl) = fetch_npm_weekly_downloads(http, pkg).await {
            analysis.npm_weekly_downloads = Some(dl);
        }
    }

    // GitHub repo metadata + README
    if let Some(repo_url) = &server.github_repo {
        if let Some((owner, repo)) = parse_github_url(repo_url) {
            if let Some(stars) = fetch_github_stars(http, &owner, &repo).await {
                analysis.github_stars = Some(stars);
            }
            if let Some(readme) = fetch_github_readme(http, &owner, &repo).await {
                analysis.extracted_addresses = extract_addresses(&readme);
                analysis.readme_excerpt = Some(readme.chars().take(800).collect());
            }
        }
    }

    analysis
}

/// Fetch weekly npm downloads. Returns None on any error.
async fn fetch_npm_weekly_downloads(http: &reqwest::Client, pkg: &str) -> Option<u64> {
    let url = format!("https://api.npmjs.org/downloads/point/last-week/{pkg}");
    let resp = http
        .get(&url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: NpmDownloads = resp.json().await.ok()?;
    Some(body.downloads)
}

/// Fetch GitHub repo metadata, return stargazers count.
async fn fetch_github_stars(http: &reqwest::Client, owner: &str, repo: &str) -> Option<u32> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let mut req = http
        .get(&url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header("Accept", "application/vnd.github+json");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            req = req.bearer_auth(token);
        }
    }
    let resp = req.send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: GithubRepo = resp.json().await.ok()?;
    Some(body.stargazers_count)
}

/// Fetch the raw README text. Tries `main` then `master`. Returns None on any
/// failure (private repo, missing README, network error, etc.).
async fn fetch_github_readme(http: &reqwest::Client, owner: &str, repo: &str) -> Option<String> {
    for branch in ["main", "master"] {
        for filename in ["README.md", "Readme.md", "readme.md", "README"] {
            let url =
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{filename}");
            let resp = http
                .get(&url)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .send()
                .await
                .ok()?;
            if resp.status().is_success() {
                return resp.text().await.ok();
            }
        }
    }
    None
}

/// Parse a GitHub URL into (owner, repo). Tolerates trailing `.git`, query
/// strings, and a few common URL shapes the registry uses.
pub fn parse_github_url(url: &str) -> Option<(String, String)> {
    // Strip protocol + host
    let stripped = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("github.com/")
        .trim_end_matches('/');
    // Drop trailing .git
    let stripped = stripped.trim_end_matches(".git");
    // Split query string off
    let stripped = stripped.split('?').next()?;
    // Take just the first two segments — anything beyond owner/repo is a sub-path
    let mut parts = stripped.split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

/// Walk a README and extract the Solana program IDs and EVM contract addresses
/// it mentions. Pure function — used for tests. Stops at 20 addresses to bound
/// memory on huge READMEs that mention many.
pub fn extract_addresses(readme: &str) -> Vec<ExtractedAddress> {
    let mut out = Vec::new();
    const MAX: usize = 20;

    // Solana program IDs are base58 strings between 32 and 44 chars (typically
    // 43-44). We do a manual scan rather than a regex crate dep — walk the
    // text, find runs of base58 chars, check the length window.
    let bytes = readme.as_bytes();
    let mut i = 0;
    while i < bytes.len() && out.len() < MAX {
        if !is_base58(bytes[i]) {
            i = i.saturating_add(1);
            continue;
        }
        let start = i;
        while i < bytes.len() && is_base58(bytes[i]) {
            i = i.saturating_add(1);
        }
        let len = i.saturating_sub(start);
        if (32..=44).contains(&len) {
            // Word-boundary check on both sides — reject if surrounded by alnum
            // (in case the run is part of a larger token).
            let before_ok = start == 0 || !bytes[start.saturating_sub(1)].is_ascii_alphanumeric();
            let after_ok = i >= bytes.len() || !bytes[i].is_ascii_alphanumeric();
            if before_ok && after_ok {
                // Heuristic: skip if the candidate looks like a long word made
                // of all-letters (no digits). Real base58 addresses have a
                // mix. Avoids false positives like "ThisIsAReallyLongWord".
                let slice = &readme[start..i];
                if slice.chars().any(|c| c.is_ascii_digit())
                    && slice.chars().any(|c| c.is_ascii_uppercase())
                    && slice.chars().any(|c| c.is_ascii_lowercase())
                {
                    out.push(ExtractedAddress {
                        kind: "solana_program".to_string(),
                        address: slice.to_string(),
                        context: context_window(readme, start, i, 80),
                    });
                }
            }
        }
    }

    // EVM contracts: 0x followed by 40 hex chars. Same word-boundary semantics.
    let mut i: usize = 0;
    while i.saturating_add(42) <= bytes.len() && out.len() < MAX {
        let next_idx = i.saturating_add(1);
        if bytes[i] == b'0' && next_idx < bytes.len() && bytes[next_idx] == b'x' {
            let after = i.saturating_add(2);
            let end = after.saturating_add(40);
            if end <= bytes.len() && (after..end).all(|j| (bytes[j] as char).is_ascii_hexdigit()) {
                let after_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
                let before_ok = i == 0 || !bytes[i.saturating_sub(1)].is_ascii_alphanumeric();
                if after_ok && before_ok {
                    out.push(ExtractedAddress {
                        kind: "evm_contract".to_string(),
                        address: readme[i..end].to_string(),
                        context: context_window(readme, i, end, 80),
                    });
                    i = end;
                    continue;
                }
            }
        }
        i = i.saturating_add(1);
    }

    out
}

fn is_base58(b: u8) -> bool {
    // base58 = 1-9, A-H, J-N, P-Z, a-k, m-z (no 0, O, I, l)
    matches!(b, b'1'..=b'9' | b'A'..=b'H' | b'J'..=b'N' | b'P'..=b'Z' | b'a'..=b'k' | b'm'..=b'z')
}

fn context_window(s: &str, start: usize, end: usize, radius: usize) -> String {
    let from = start.saturating_sub(radius);
    let to = end.saturating_add(radius).min(s.len());
    // Snap to char boundaries to avoid panicking inside multi-byte UTF-8.
    let mut from = from;
    while from > 0 && !s.is_char_boundary(from) {
        from = from.saturating_sub(1);
    }
    let mut to = to;
    while to < s.len() && !s.is_char_boundary(to) {
        to = to.saturating_add(1);
    }
    s[from..to].replace('\n', " ").trim().to_string()
}

/// Result of one Layer 3 pass.
#[derive(Debug, serde::Serialize)]
pub struct DeepAnalysisSummary {
    pub considered: usize,
    pub probed: usize,
    /// Servers we couldn't analyze because they had neither npm pkg nor GitHub repo.
    pub skipped: usize,
    pub addresses_found: usize,
    pub firestore_writes: usize,
    pub firestore_write_errors: usize,
    pub elapsed_ms: u64,
}

/// Top-level entry point — pulls candidates from Firestore via the discovery
/// state, picks the top N, runs deep analysis on each, and invalidates the
/// in-memory cache so the next earning-candidates query sees the new
/// `layer3_analysis` fields.
pub async fn run_layer3_pass(
    state: &Arc<crate::discovery::DiscoveryState>,
) -> Result<DeepAnalysisSummary> {
    let all_servers = crate::discovery::load_from_firestore(&state.db)
        .await
        .context("load discovery index for layer 3")?;
    let candidates = select_top_candidates(all_servers, MAX_SERVERS_PER_CYCLE);
    let summary = run_deep_analysis(&state.db, &state.http, &candidates).await?;

    // Invalidate the cache so subsequent /earning-candidates and /primitives
    // calls re-read from Firestore and pick up the new Layer 3 fields.
    {
        let mut cache = state.cache.lock().await;
        *cache = None;
    }

    Ok(summary)
}

// -- Wire types for npm + GitHub responses --

#[derive(Debug, Deserialize)]
struct NpmDownloads {
    downloads: u64,
}

#[derive(Debug, Deserialize)]
struct GithubRepo {
    stargazers_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_url_https_with_git_suffix() {
        assert_eq!(
            parse_github_url("https://github.com/foo/bar.git"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn parse_github_url_https_no_suffix() {
        assert_eq!(
            parse_github_url("https://github.com/foo/bar"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn parse_github_url_with_subpath_truncates_to_owner_repo() {
        assert_eq!(
            parse_github_url("https://github.com/foo/bar/tree/main/packages/server"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn parse_github_url_handles_trailing_slash() {
        assert_eq!(
            parse_github_url("https://github.com/foo/bar/"),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn parse_github_url_rejects_garbage() {
        assert_eq!(parse_github_url(""), None);
        assert_eq!(parse_github_url("https://github.com/"), None);
        assert_eq!(parse_github_url("not a url at all"), None);
    }

    #[test]
    fn extract_addresses_finds_solana_program() {
        let text =
            "Our program is deployed at 2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P on mainnet.";
        let addrs = extract_addresses(text);
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].kind, "solana_program");
        assert_eq!(
            addrs[0].address,
            "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"
        );
        assert!(addrs[0]
            .context
            .contains("2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"));
    }

    #[test]
    fn extract_addresses_finds_evm_contract() {
        let text = "USDC on Base is at 0x833589fcd6edb6e08f4c7c32d4f71b54bda02913.";
        let addrs = extract_addresses(text);
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].kind, "evm_contract");
        assert_eq!(
            addrs[0].address,
            "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"
        );
    }

    #[test]
    fn extract_addresses_finds_both_kinds() {
        let text = "Program 2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P on Solana, contract 0x833589fcd6edb6e08f4c7c32d4f71b54bda02913 on Base.";
        let addrs = extract_addresses(text);
        assert_eq!(addrs.len(), 2);
        let kinds: Vec<&str> = addrs.iter().map(|a| a.kind.as_str()).collect();
        assert!(kinds.contains(&"solana_program"));
        assert!(kinds.contains(&"evm_contract"));
    }

    #[test]
    fn extract_addresses_skips_all_letters_runs() {
        // Long all-letter words shouldn't get flagged as program IDs.
        let text = "ThisIsAReallyLongCamelCaseIdentifierWithoutAnyDigits";
        let addrs = extract_addresses(text);
        assert!(
            addrs.is_empty(),
            "all-letter run should not be a Solana address"
        );
    }

    #[test]
    fn extract_addresses_caps_at_max() {
        // Construct a README with many real addresses, ensure we cap at 20.
        let mut text = String::new();
        for _ in 0..30 {
            text.push_str("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913 ");
        }
        let addrs = extract_addresses(&text);
        assert!(addrs.len() <= 20);
    }

    #[test]
    fn select_top_candidates_sorts_by_source_count_then_name() {
        use crate::discovery::models::{
            CashFlowDirection, Layer1Classification, RawServer, ValueToSwarm,
        };
        let now = chrono::Utc::now();
        fn fake(name: &str, source_count: u32) -> EnrichedServer {
            EnrichedServer {
                name: name.to_string(),
                title: None,
                description: None,
                endpoint: None,
                transport: None,
                npm_package: None,
                github_repo: None,
                sources: vec!["x".into()],
                source_count,
                upstream_quality_score: None,
                upstream_visitors_estimate: None,
                classification: Layer1Classification {
                    category: None,
                    cash_flow_direction: Some(CashFlowDirection::EarnsForAgent),
                    currencies: vec![],
                    value_to_swarm: Some(ValueToSwarm::AggregateListing),
                    confident: true,
                    matched_signals: vec!["earning_keyword".into()],
                },
                layer2_classification: None,
                layer3_analysis: None,
                first_seen_at: chrono::Utc::now(),
                last_seen_at: chrono::Utc::now(),
            }
        }
        let _ = (now, fake("ignore", 0)); // silence unused fn warning if compiler optimizes

        let servers = vec![
            fake("zzz", 1),
            fake("aaa", 3),
            fake("bbb", 3),
            fake("ccc", 2),
        ];
        let _ = RawServer {
            name: "ignored".into(),
            title: None,
            description: None,
            endpoint: None,
            transport: None,
            npm_package: None,
            github_repo: None,
            source: "x".into(),
            upstream_quality_score: None,
            upstream_visitors_estimate: None,
        };
        let top = select_top_candidates(servers, 3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].name, "aaa"); // 3 sources, alphabetically first
        assert_eq!(top[1].name, "bbb"); // 3 sources, second alphabetically
        assert_eq!(top[2].name, "ccc"); // 2 sources
    }
}
