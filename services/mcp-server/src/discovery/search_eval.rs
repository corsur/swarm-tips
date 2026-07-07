//! Retrieval-quality eval harness for `discovery::search`.
//!
//! The engine panel's #1 required change: no ranking knob gets tuned by
//! argument — NDCG@5 / MRR run in CI over a labeled fixture, and
//! **reorder-delta** (the % of queries where the automated quality prior
//! changes the top-5 vs pure BM25) proves the ranking signals actually do
//! something without letting them overwhelm relevance.
//!
//! The fixture is a deterministic synthetic catalog shaped like the real
//! ingested corpus (names/descriptions modeled on live entries, mixed
//! quality signals). Thresholds are set just below currently-achieved
//! values — they exist to catch regressions, not to flatter the ranker.

use super::models::{
    CashFlowDirection, Category, EnrichedServer, Layer1Classification, Layer3Analysis, ValueToSwarm,
};
use super::search::{SearchFilters, SearchIndex};

/// Fixture server builder with adjustable automated signals.
#[allow(clippy::too_many_arguments)]
fn server(
    name: &str,
    description: &str,
    category: Option<Category>,
    currencies: &[&str],
    source_count: u32,
    stars: Option<u32>,
    downloads: Option<u64>,
) -> EnrichedServer {
    EnrichedServer {
        name: name.to_string(),
        title: None,
        description: Some(description.to_string()),
        endpoint: None,
        transport: None,
        npm_package: None,
        github_repo: None,
        sources: vec!["official".to_string()],
        source_count,
        upstream_quality_score: None,
        upstream_visitors_estimate: None,
        classification: Layer1Classification {
            category,
            cash_flow_direction: Some(CashFlowDirection::Neutral),
            currencies: currencies.iter().map(|c| c.to_string()).collect(),
            value_to_swarm: Some(ValueToSwarm::None),
            confident: true,
            matched_signals: vec![],
        },
        layer2_classification: None,
        layer3_analysis: if stars.is_some() || downloads.is_some() {
            Some(Layer3Analysis {
                extracted_addresses: vec![],
                npm_weekly_downloads: downloads,
                github_stars: stars,
                readme_excerpt: None,
                probed: true,
                probed_at: chrono::Utc::now(),
            })
        } else {
            None
        },
        first_seen_at: chrono::Utc::now(),
        last_seen_at: chrono::Utc::now(),
    }
}

/// The labeled fixture catalog: ~30 servers across the capability space,
/// with distractors. Signal-rich and signal-poor variants deliberately
/// coexist so reorder-delta has something to measure.
fn fixture_catalog() -> Vec<EnrichedServer> {
    use Category::*;
    vec![
        // Solana / DeFi cluster
        server(
            "jupiter/swap-mcp",
            "Swap tokens on Solana via Jupiter aggregator, best-route USDC SOL trades",
            Some(Payment),
            &["SOL", "USDC"],
            3,
            Some(2_000),
            Some(20_000),
        ),
        server(
            "raydium/dex-tools",
            "Solana DEX liquidity pools and swap execution",
            Some(Payment),
            &["SOL"],
            1,
            None,
            None,
        ),
        server(
            "obscure/sol-swapper",
            "Swap tokens on Solana",
            Some(Payment),
            &["SOL"],
            1,
            None,
            None,
        ),
        server(
            "eth/uniswap-mcp",
            "Uniswap v4 swaps and liquidity on Ethereum and Base",
            Some(Payment),
            &["ETH", "USDC"],
            2,
            Some(5_000),
            None,
        ),
        // Browser automation cluster
        server(
            "playwright/browser-mcp",
            "Browser automation: navigate, click, fill forms, screenshot pages with Playwright",
            Some(Devtools),
            &[],
            4,
            Some(12_000),
            Some(90_000),
        ),
        server(
            "tiny/browser-driver",
            "Automate a browser: click and screenshot",
            Some(Devtools),
            &[],
            1,
            None,
            None,
        ),
        // Data / search cluster
        server(
            "exa/web-search-mcp",
            "Web search API for agents: semantic queries, news, papers",
            Some(Data),
            &[],
            3,
            Some(3_000),
            Some(40_000),
        ),
        server(
            "duck/search-lite",
            "Search the web",
            Some(Data),
            &[],
            1,
            None,
            None,
        ),
        server(
            "postgres/db-mcp",
            "Query PostgreSQL databases, inspect schemas, run SQL",
            Some(Data),
            &[],
            3,
            Some(8_000),
            Some(60_000),
        ),
        server(
            "sqlite/local-db",
            "SQLite database queries",
            Some(Data),
            &[],
            2,
            None,
            None,
        ),
        // Payments cluster
        server(
            "stripe/payments-mcp",
            "Create Stripe payment links, invoices, and refunds",
            Some(Payment),
            &["USD"],
            3,
            Some(4_000),
            Some(30_000),
        ),
        server(
            "x402/paywall-mcp",
            "x402 payment-required flows: pay per request with USDC on Base and Solana",
            Some(Payment),
            &["USDC"],
            2,
            Some(500),
            None,
        ),
        // Content / social cluster
        server(
            "youtube/video-mcp",
            "Search YouTube videos, fetch transcripts and channel stats",
            Some(Content),
            &[],
            2,
            Some(1_500),
            Some(15_000),
        ),
        server(
            "x-twitter/post-mcp",
            "Post tweets, read timelines, manage X threads",
            Some(Social),
            &[],
            2,
            Some(900),
            None,
        ),
        // Bounty / earning cluster
        server(
            "bounty/agent-jobs-mcp",
            "Browse and claim crypto bounties for AI agents, USDC escrow payouts",
            Some(Bounty),
            &["USDC"],
            2,
            Some(300),
            None,
        ),
        server(
            "gig/task-board",
            "Task board for agent gigs and bounties",
            Some(Bounty),
            &["USDC"],
            1,
            None,
            None,
        ),
        // Infra cluster
        server(
            "k8s/cluster-mcp",
            "Manage Kubernetes clusters: pods, deployments, logs",
            Some(Infrastructure),
            &[],
            3,
            Some(6_000),
            Some(25_000),
        ),
        server(
            "aws/cloud-mcp",
            "AWS operations: S3, EC2, Lambda management",
            Some(Infrastructure),
            &[],
            3,
            Some(7_000),
            Some(35_000),
        ),
        // Devtools cluster
        server(
            "github/repo-mcp",
            "GitHub repos: issues, PRs, code search, commits",
            Some(Devtools),
            &[],
            4,
            Some(15_000),
            Some(120_000),
        ),
        server(
            "git/local-git",
            "Local git operations",
            Some(Devtools),
            &[],
            2,
            None,
            None,
        ),
        // Weather / misc distractors
        server(
            "weather/forecast-mcp",
            "Weather forecasts and severe-weather alerts by location",
            Some(Data),
            &[],
            2,
            Some(400),
            None,
        ),
        server(
            "pdf/document-mcp",
            "Extract text and tables from PDF documents",
            Some(Data),
            &[],
            2,
            Some(1_100),
            Some(9_000),
        ),
        server(
            "email/smtp-mcp",
            "Send and read email over SMTP/IMAP",
            Some(Social),
            &[],
            2,
            Some(700),
            None,
        ),
        server(
            "calendar/schedule-mcp",
            "Google Calendar events: create, list, update",
            Some(Data),
            &[],
            2,
            Some(600),
            None,
        ),
        server(
            "translate/lang-mcp",
            "Translate text between 100+ languages",
            Some(Data),
            &[],
            1,
            None,
            None,
        ),
        server(
            "crypto/price-feed",
            "Live cryptocurrency prices and market data, SOL ETH BTC",
            Some(Data),
            &["SOL", "ETH"],
            2,
            Some(800),
            None,
        ),
        server(
            "nft/mint-mcp",
            "Mint and list NFTs on Solana",
            Some(Payment),
            &["SOL"],
            1,
            None,
            None,
        ),
        server(
            "game/chess-mcp",
            "Play chess games against engines",
            Some(Game),
            &[],
            1,
            None,
            None,
        ),
        server(
            "audio/tts-mcp",
            "Text-to-speech audio generation",
            Some(Content),
            &[],
            2,
            Some(1_000),
            Some(12_000),
        ),
        server(
            "image/gen-mcp",
            "Generate images from text prompts",
            Some(Content),
            &[],
            3,
            Some(2_500),
            Some(22_000),
        ),
    ]
}

/// Labeled queries: (query, [(relevant server name, graded relevance)]).
/// Grades: 2 = highly relevant, 1 = partially relevant.
fn labeled_queries() -> Vec<(&'static str, Vec<(&'static str, f64)>)> {
    vec![
        (
            "solana token swap",
            vec![
                ("jupiter/swap-mcp", 2.0),
                ("raydium/dex-tools", 2.0),
                ("obscure/sol-swapper", 2.0),
                ("nft/mint-mcp", 1.0),
            ],
        ),
        (
            "browser automation click screenshot",
            vec![
                ("playwright/browser-mcp", 2.0),
                ("tiny/browser-driver", 2.0),
            ],
        ),
        (
            "web search for agents",
            vec![("exa/web-search-mcp", 2.0), ("duck/search-lite", 2.0)],
        ),
        (
            "query postgres database sql",
            vec![("postgres/db-mcp", 2.0), ("sqlite/local-db", 1.0)],
        ),
        ("stripe payment invoice", vec![("stripe/payments-mcp", 2.0)]),
        (
            "pay per request usdc x402",
            vec![("x402/paywall-mcp", 2.0), ("bounty/agent-jobs-mcp", 1.0)],
        ),
        ("youtube video transcript", vec![("youtube/video-mcp", 2.0)]),
        ("post tweets to x", vec![("x-twitter/post-mcp", 2.0)]),
        (
            "claim crypto bounties usdc",
            vec![("bounty/agent-jobs-mcp", 2.0), ("gig/task-board", 2.0)],
        ),
        (
            "kubernetes pods deployments",
            vec![("k8s/cluster-mcp", 2.0)],
        ),
        ("aws s3 lambda", vec![("aws/cloud-mcp", 2.0)]),
        (
            "github issues pull requests",
            vec![("github/repo-mcp", 2.0), ("git/local-git", 1.0)],
        ),
        (
            "weather forecast alerts",
            vec![("weather/forecast-mcp", 2.0)],
        ),
        ("extract text from pdf", vec![("pdf/document-mcp", 2.0)]),
        ("send email", vec![("email/smtp-mcp", 2.0)]),
        ("calendar events", vec![("calendar/schedule-mcp", 2.0)]),
        ("translate languages", vec![("translate/lang-mcp", 2.0)]),
        (
            "cryptocurrency prices market data",
            vec![("crypto/price-feed", 2.0)],
        ),
        (
            "mint nft solana",
            vec![("nft/mint-mcp", 2.0), ("jupiter/swap-mcp", 1.0)],
        ),
        ("text to speech audio", vec![("audio/tts-mcp", 2.0)]),
        ("generate images from prompts", vec![("image/gen-mcp", 2.0)]),
        ("uniswap ethereum liquidity", vec![("eth/uniswap-mcp", 2.0)]),
        ("play chess", vec![("game/chess-mcp", 2.0)]),
        (
            "dex liquidity pools",
            vec![("raydium/dex-tools", 2.0), ("eth/uniswap-mcp", 2.0)],
        ),
    ]
}

/// DCG with graded relevance over the top-k returned names.
fn dcg_at_k(returned: &[String], labels: &[(&str, f64)], k: usize) -> f64 {
    returned
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, name)| {
            let rel = labels
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, g)| *g)
                .unwrap_or(0.0);
            let denom = ((i as f64) + 2.0).log2();
            (2f64.powf(rel) - 1.0) / denom
        })
        .sum()
}

fn ndcg_at_k(returned: &[String], labels: &[(&str, f64)], k: usize) -> f64 {
    let mut ideal: Vec<f64> = labels.iter().map(|(_, g)| *g).collect();
    ideal.sort_by(|a, b| b.total_cmp(a));
    let idcg: f64 = ideal
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, rel)| (2f64.powf(*rel) - 1.0) / (((i as f64) + 2.0).log2()))
        .sum();
    if idcg == 0.0 {
        return 0.0;
    }
    dcg_at_k(returned, labels, k) / idcg
}

/// Reciprocal rank of the first relevant hit.
fn reciprocal_rank(returned: &[String], labels: &[(&str, f64)]) -> f64 {
    for (i, name) in returned.iter().enumerate() {
        if labels.iter().any(|(n, _)| n == name) {
            return 1.0 / ((i as f64) + 1.0);
        }
    }
    0.0
}

fn top_names(index: &SearchIndex, query: &str, k: usize) -> Vec<String> {
    index
        .search(query, &SearchFilters::default(), k)
        .into_iter()
        .map(|h| h.name)
        .collect()
}

/// Signal-stripped clone of the catalog: identical text, uniform automated
/// signals → uniform quality prior → ordering is pure BM25. The baseline
/// for reorder-delta.
fn strip_signals(catalog: &[EnrichedServer]) -> Vec<EnrichedServer> {
    catalog
        .iter()
        .map(|s| {
            let mut c = s.clone();
            c.source_count = 1;
            c.upstream_quality_score = None;
            c.layer2_classification = None;
            if let Some(l3) = &mut c.layer3_analysis {
                l3.github_stars = None;
                l3.npm_weekly_downloads = None;
            }
            c
        })
        .collect()
}

#[test]
fn eval_ndcg_at_5_meets_floor() {
    let index = SearchIndex::build(fixture_catalog(), chrono::Utc::now());
    let queries = labeled_queries();
    let mut total = 0.0;
    let mut worst: (f64, &str) = (f64::MAX, "");
    for (q, labels) in &queries {
        let ndcg = ndcg_at_k(&top_names(&index, q, 5), labels, 5);
        if ndcg < worst.0 {
            worst = (ndcg, q);
        }
        total += ndcg;
    }
    let mean = total / queries.len() as f64;
    // Floor set just below achieved (regression tripwire, not flattery).
    assert!(
        mean >= 0.80,
        "mean NDCG@5 regressed: {mean:.3} (worst query: '{}' at {:.3})",
        worst.1,
        worst.0
    );
}

#[test]
fn eval_mrr_meets_floor() {
    let index = SearchIndex::build(fixture_catalog(), chrono::Utc::now());
    let queries = labeled_queries();
    let mrr: f64 = queries
        .iter()
        .map(|(q, labels)| reciprocal_rank(&top_names(&index, q, 10), labels))
        .sum::<f64>()
        / queries.len() as f64;
    assert!(mrr >= 0.85, "MRR regressed: {mrr:.3}");
}

#[test]
fn eval_reorder_delta_prior_active_but_not_dominant() {
    let with_signals = SearchIndex::build(fixture_catalog(), chrono::Utc::now());
    let baseline = SearchIndex::build(strip_signals(&fixture_catalog()), chrono::Utc::now());
    let queries = labeled_queries();
    let mut reordered = 0usize;
    for (q, _) in &queries {
        if top_names(&with_signals, q, 5) != top_names(&baseline, q, 5) {
            reordered += 1;
        }
    }
    let delta = reordered as f64 / queries.len() as f64;
    // The prior must DO something (>0) but relevance must stay dominant
    // (engine-panel F1) — if most top-5s change, quality is overwhelming
    // relevance and the bounds need retuning.
    assert!(delta > 0.0, "quality prior is inert — reorder-delta is 0");
    assert!(
        delta <= 0.5,
        "quality prior dominates relevance: reorder-delta {delta:.2} > 0.5"
    );
}

#[test]
fn eval_keyword_stuffing_does_not_beat_genuine_signal() {
    // Anti-gaming: a spam server stuffing the query terms with zero
    // machine signals must not outrank a genuine, signal-rich server
    // that mentions the capability naturally.
    let mut catalog = fixture_catalog();
    catalog.push(server(
        "spam/stuffed-swapper",
        "solana swap solana swap solana swap token swap solana token swap swap tokens solana best solana swap swap swap solana",
        Some(Category::Payment),
        &["SOL"],
        1,
        None,
        None,
    ));
    let index = SearchIndex::build(catalog, chrono::Utc::now());
    let top = top_names(&index, "solana token swap", 3);
    assert_eq!(
        top[0], "jupiter/swap-mcp",
        "genuine signal-rich server must outrank the keyword-stuffed spam entry; got {top:?}"
    );
}
