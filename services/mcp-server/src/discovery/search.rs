//! BM25 search over the full ingested MCP-server catalog.
//!
//! Replaces the retired hand-curated `layer3` substring directory (30
//! entries) with relevance search over every server the discovery
//! pipeline ingests (~2k from the official registry + awesome-lists).
//!
//! Ranking contract (locked 2026-07-07): **fully automated — no manual
//! curation influences the order.** BM25 relevance gates inclusion;
//! survivors are ordered by `bm25_score × quality_prior`, where the
//! prior is computed only from machine-generated signals already in the
//! pipeline: multi-source corroboration (`source_count`), GitHub stars +
//! npm weekly downloads (Layer 3 deep-analysis), upstream quality score
//! (best-of-mcp), and Layer 2 LLM classification confidence.
//! First-party detection is a fact (endpoint/repo domain), not a
//! judgment. Every hit discloses its ranking signals for auditability.
//!
//! ```text
//! DiscoveryCache (Vec<EnrichedServer>)
//!        │  build_index() — on refresh_discovery / lazily on first search
//!        ▼
//! SearchIndex { bm25 engine ── docs by ordinal ── quality priors }
//!        │  search(query, filters, limit)
//!        ▼
//! relevance gate (BM25 match) ─► score × prior ─► filters ─► top-N hits
//! ```

use crate::discovery::models::EnrichedServer;
use bm25::{Document, Language, SearchEngine, SearchEngineBuilder};
use serde::Serialize;

/// Domains that make a server first-party. A fact about where the
/// endpoint/repo lives — not a curation judgment.
const FIRST_PARTY_MARKERS: [&str; 5] = [
    "swarm.tips",
    "coordination.game",
    "shillbot.org",
    "github.com/corsur",
    // The MCP registry's reverse-DNS server-name form. It used to be ORed in
    // separately at the is_first_party call site, outside the constant this doc
    // says is the list — so adding a marker here silently missed it.
    "io.github.corsur",
];

/// Candidate multiplier: how many BM25 hits to pull before filtering and
/// prior-reranking. Bounded so a huge `limit` cannot make the engine scan
/// unboundedly.
const CANDIDATE_FACTOR: usize = 4;
const MAX_CANDIDATES: usize = 400;

/// In-memory search index over one snapshot of the catalog. Immutable
/// once built — rebuilt wholesale when the catalog refreshes.
pub struct SearchIndex {
    engine: SearchEngine<u32>,
    servers: Vec<EnrichedServer>,
    priors: Vec<f64>,
    /// When the underlying catalog snapshot was ingested (cache
    /// `last_refreshed_at`) — surfaced to callers for staleness
    /// disclosure.
    pub catalog_refreshed_at: chrono::DateTime<chrono::Utc>,
}

/// One search hit with its full automated-signal breakdown.
#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub transport: Option<String>,
    pub npm_package: Option<String>,
    pub github_repo: Option<String>,
    pub category: Option<String>,
    pub currencies: Vec<String>,
    /// "first-party" (endpoint/repo on a swarm.tips-operated domain) or
    /// "external". Automated provenance, not a vetting tier.
    pub provenance: &'static str,
    pub ranking_signals: RankingSignals,
}

/// The per-hit ranking disclosure. Everything here is machine-generated.
#[derive(Debug, Serialize)]
pub struct RankingSignals {
    pub bm25_score: f64,
    pub source_count: u32,
    pub github_stars: Option<u32>,
    pub npm_weekly_downloads: Option<u64>,
    pub upstream_quality_score: Option<f32>,
    pub llm_confidence: Option<f32>,
    pub quality_prior: f64,
    pub final_score: f64,
}

/// Filters applied after the relevance gate.
#[derive(Debug, Default)]
pub struct SearchFilters<'a> {
    pub category: Option<&'a str>,
    pub currency: Option<&'a str>,
    /// "first-party" | "external" (anything else returns zero hits).
    pub provenance: Option<&'a str>,
}

impl SearchIndex {
    /// Build the index from a catalog snapshot. O(corpus) — ~2k docs is
    /// milliseconds; called on refresh, never per-request.
    pub fn build(
        servers: Vec<EnrichedServer>,
        catalog_refreshed_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        // Precondition: ordinals fit the u32 doc-id space.
        assert!(
            servers.len() < u32::MAX as usize,
            "catalog cannot exceed u32 ordinal space"
        );
        let docs: Vec<Document<u32>> = servers
            .iter()
            .enumerate()
            .map(|(i, s)| Document::new(i as u32, index_text(s)))
            .collect();
        let priors: Vec<f64> = servers.iter().map(quality_prior).collect();
        // Postcondition: parallel vectors stay aligned.
        assert_eq!(servers.len(), priors.len(), "priors must align to servers");
        let engine = SearchEngineBuilder::<u32>::with_documents(Language::English, docs).build();
        Self {
            engine,
            servers,
            priors,
            catalog_refreshed_at,
        }
    }

    /// Number of servers in the indexed snapshot.
    pub fn corpus_size(&self) -> usize {
        self.servers.len()
    }

    /// Relevance-gated, prior-reranked search.
    ///
    /// With a non-empty `query`, only BM25 matches are eligible (a
    /// nonsense query returns nothing). With an empty query, the filtered
    /// corpus is browsed in quality-prior order — a discovery mode, still
    /// fully automated.
    pub fn search(&self, query: &str, filters: &SearchFilters, limit: usize) -> Vec<SearchHit> {
        let limit = limit.min(200);
        let candidates: Vec<(u32, f64)> = if query.trim().is_empty() {
            // Browse mode: whole corpus, neutral relevance.
            (0..self.servers.len() as u32).map(|i| (i, 1.0)).collect()
        } else {
            let want = (limit.saturating_mul(CANDIDATE_FACTOR)).min(MAX_CANDIDATES);
            self.engine
                .search(query, want)
                .into_iter()
                .map(|r| (r.document.id, f64::from(r.score)))
                .collect()
        };

        let mut hits: Vec<(f64, SearchHit)> = candidates
            .into_iter()
            .filter_map(|(id, bm25_score)| {
                let server = self.servers.get(id as usize)?;
                let prior = *self.priors.get(id as usize)?;
                if !matches_filters(server, filters) {
                    return None;
                }
                let final_score = bm25_score * prior;
                Some((final_score, to_hit(server, bm25_score, prior, final_score)))
            })
            .collect();

        hits.sort_by(|a, b| b.0.total_cmp(&a.0));
        hits.truncate(limit);
        hits.into_iter().map(|(_, h)| h).collect()
    }
}

/// The indexed text for one server: every automated textual field the
/// pipeline produces. Hand-written curation text is intentionally absent.
fn index_text(s: &EnrichedServer) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(8);
    parts.push(s.name.clone());
    if let Some(t) = &s.title {
        parts.push(t.clone());
    }
    if let Some(d) = &s.description {
        parts.push(d.clone());
    }
    if let Some(c) = effective_category(s) {
        parts.push(c);
    }
    let currencies = effective_currencies(s);
    if !currencies.is_empty() {
        parts.push(currencies.join(" "));
    }
    if let Some(l3) = &s.layer3_analysis {
        if let Some(readme) = &l3.readme_excerpt {
            parts.push(readme.clone());
        }
    }
    parts.join("\n")
}

/// Quality prior from automated signals only. Neutral = 1.0, boosts to
/// 2.0 max so relevance stays dominant (engine-panel F1: relevance gates,
/// quality reorders — never overwhelms). Degenerate repetitive text
/// (keyword stuffing) is penalized below 1.0 by the content-diversity
/// factor — also machine-derived, no curation.
fn quality_prior(s: &EnrichedServer) -> f64 {
    let mut q = 1.0_f64;
    // Multi-source corroboration: 1 source → +0.10, 4 sources → +0.24.
    q += 0.15 * f64::from(s.source_count).ln_1p();
    if let Some(l3) = &s.layer3_analysis {
        if let Some(stars) = l3.github_stars {
            q += (0.05 * f64::from(stars).ln_1p() / std::f64::consts::LN_10).min(0.25);
        }
        if let Some(dl) = l3.npm_weekly_downloads {
            q += (0.05 * (dl as f64).ln_1p() / std::f64::consts::LN_10).min(0.25);
        }
    }
    if let Some(uq) = s.upstream_quality_score {
        q += 0.2 * f64::from(uq.clamp(0.0, 1.0));
    }
    if let Some(l2) = &s.layer2_classification {
        q += 0.1 * f64::from(l2.confidence.clamp(0.0, 1.0));
    }
    q *= diversity_factor(s);
    let q = q.clamp(0.1, 2.0);
    // Postcondition: bounded so neither spam-burial nor popularity can
    // overwhelm the BM25 relevance gate.
    assert!((0.1..=2.0).contains(&q), "quality prior out of bounds");
    q
}

/// Content-diversity factor: keyword-stuffed descriptions repeat a handful
/// of tokens, so their unique-token ratio collapses while natural prose
/// stays high. Quadratic penalty below the 0.5 ratio; short texts (where a
/// low ratio is not evidence of stuffing) are exempt. Fully automated —
/// derived from the server's own published text.
fn diversity_factor(s: &EnrichedServer) -> f64 {
    let text = format!(
        "{} {} {}",
        s.name,
        s.title.as_deref().unwrap_or(""),
        s.description.as_deref().unwrap_or("")
    )
    .to_lowercase();
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() < 12 {
        return 1.0;
    }
    let unique: std::collections::HashSet<&str> = tokens.iter().copied().collect();
    let ratio = unique.len() as f64 / tokens.len() as f64;
    if ratio >= 0.5 {
        1.0
    } else {
        (ratio / 0.5).powi(2)
    }
}

/// First-party is a fact about where the server lives, not a judgment.
fn is_first_party(s: &EnrichedServer) -> bool {
    let fields = [
        s.endpoint.as_deref().unwrap_or(""),
        s.github_repo.as_deref().unwrap_or(""),
        s.name.as_str(),
    ];
    fields.iter().any(|f| {
        let f = f.to_lowercase();
        FIRST_PARTY_MARKERS.iter().any(|m| f.contains(m))
    })
}

/// Category with Layer-2 fallback (Layer 1 stays authoritative when set).
fn effective_category(s: &EnrichedServer) -> Option<String> {
    let l1 = s.classification.category;
    let l2 = s.layer2_classification.as_ref().and_then(|c| c.category);
    l1.or(l2).map(|c| format!("{c:?}").to_lowercase())
}

/// Currencies merged across Layer 1 + Layer 2 (deduped, lowercased).
fn effective_currencies(s: &EnrichedServer) -> Vec<String> {
    let mut out: Vec<String> = s
        .classification
        .currencies
        .iter()
        .chain(
            s.layer2_classification
                .iter()
                .flat_map(|c| c.currencies.iter()),
        )
        .map(|c| c.to_lowercase())
        .collect();
    out.sort();
    out.dedup();
    out
}

fn matches_filters(s: &EnrichedServer, filters: &SearchFilters) -> bool {
    if let Some(cat) = filters.category {
        let cat = cat.to_lowercase();
        match effective_category(s) {
            Some(c) if c.contains(&cat) => {}
            _ => return false,
        }
    }
    if let Some(cur) = filters.currency {
        let cur = cur.to_lowercase();
        if !effective_currencies(s).iter().any(|c| c.contains(&cur)) {
            return false;
        }
    }
    if let Some(p) = filters.provenance {
        let first_party = is_first_party(s);
        match p {
            "first-party" if first_party => {}
            "external" if !first_party => {}
            _ => return false,
        }
    }
    true
}

fn to_hit(s: &EnrichedServer, bm25_score: f64, prior: f64, final_score: f64) -> SearchHit {
    SearchHit {
        name: s.name.clone(),
        title: s.title.clone(),
        description: s.description.as_ref().map(|d| truncate(d, 400)),
        endpoint: s.endpoint.clone(),
        transport: s.transport.clone(),
        npm_package: s.npm_package.clone(),
        github_repo: s.github_repo.clone(),
        category: effective_category(s),
        currencies: effective_currencies(s),
        provenance: if is_first_party(s) {
            "first-party"
        } else {
            "external"
        },
        ranking_signals: RankingSignals {
            bm25_score,
            source_count: s.source_count,
            github_stars: s.layer3_analysis.as_ref().and_then(|l| l.github_stars),
            npm_weekly_downloads: s
                .layer3_analysis
                .as_ref()
                .and_then(|l| l.npm_weekly_downloads),
            upstream_quality_score: s.upstream_quality_score,
            llm_confidence: s.layer2_classification.as_ref().map(|c| c.confidence),
            quality_prior: prior,
            final_score,
        },
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}…", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::models::{
        CashFlowDirection, Category, Layer1Classification, Layer3Analysis, ValueToSwarm,
    };

    fn make_server(name: &str, description: &str) -> EnrichedServer {
        EnrichedServer {
            name: name.to_string(),
            title: None,
            description: Some(description.to_string()),
            endpoint: None,
            transport: None,
            npm_package: None,
            github_repo: None,
            sources: vec!["official".to_string()],
            source_count: 1,
            upstream_quality_score: None,
            upstream_visitors_estimate: None,
            classification: Layer1Classification {
                category: None,
                cash_flow_direction: None,
                currencies: vec![],
                value_to_swarm: None,
                confident: false,
                matched_signals: vec![],
            },
            layer2_classification: None,
            layer3_analysis: None,
            first_seen_at: chrono::Utc::now(),
            last_seen_at: chrono::Utc::now(),
        }
    }

    fn build(servers: Vec<EnrichedServer>) -> SearchIndex {
        SearchIndex::build(servers, chrono::Utc::now())
    }

    #[test]
    fn relevant_query_ranks_matching_server_first() {
        let idx = build(vec![
            make_server("weather-tools", "Fetch weather forecasts and alerts"),
            make_server(
                "solana-defi",
                "Swap tokens on Solana DEXes, USDC and SOL liquidity",
            ),
            make_server("pdf-reader", "Extract text from PDF documents"),
        ]);
        let hits = idx.search("solana defi swap", &SearchFilters::default(), 10);
        assert!(!hits.is_empty(), "expected at least one hit");
        assert_eq!(hits[0].name, "solana-defi");
    }

    #[test]
    fn nonsense_query_returns_empty() {
        let idx = build(vec![
            make_server("weather-tools", "Fetch weather forecasts"),
            make_server("pdf-reader", "Extract text from PDF documents"),
        ]);
        let hits = idx.search("zqxvbn floop", &SearchFilters::default(), 10);
        assert!(hits.is_empty(), "nonsense query must return nothing");
    }

    #[test]
    fn quality_prior_reorders_equal_relevance() {
        let mut popular = make_server("dex-alpha", "Solana DEX trading tools");
        popular.source_count = 4;
        popular.layer3_analysis = Some(Layer3Analysis {
            extracted_addresses: vec![],
            npm_weekly_downloads: Some(50_000),
            github_stars: Some(4_000),
            readme_excerpt: None,
            probed: true,
            probed_at: chrono::Utc::now(),
        });
        let obscure = make_server("dex-beta", "Solana DEX trading tools");
        let idx = build(vec![obscure, popular]);
        let hits = idx.search("solana dex trading", &SearchFilters::default(), 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits[0].name, "dex-alpha",
            "higher automated-quality server must rank first at equal relevance"
        );
        assert!(hits[0].ranking_signals.quality_prior > hits[1].ranking_signals.quality_prior);
    }

    #[test]
    fn first_party_provenance_is_detected_and_filterable() {
        let mut ours = make_server("io.github.corsur/swarm-tips", "Earn crypto, play games");
        ours.endpoint = Some("https://mcp.swarm.tips/mcp".to_string());
        let theirs = make_server("other/games-server", "Play games with agents");
        let idx = build(vec![ours, theirs]);

        let fp = idx.search(
            "games",
            &SearchFilters {
                provenance: Some("first-party"),
                ..Default::default()
            },
            10,
        );
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].provenance, "first-party");

        let ext = idx.search(
            "games",
            &SearchFilters {
                provenance: Some("external"),
                ..Default::default()
            },
            10,
        );
        assert_eq!(ext.len(), 1);
        assert_eq!(ext[0].provenance, "external");
    }

    #[test]
    fn category_and_currency_filters_apply() {
        let mut bounty = make_server("bounty-board", "Claim bounties for USDC rewards");
        bounty.classification = Layer1Classification {
            category: Some(Category::Bounty),
            cash_flow_direction: Some(CashFlowDirection::EarnsForAgent),
            currencies: vec!["USDC".to_string()],
            value_to_swarm: Some(ValueToSwarm::AggregateListing),
            confident: true,
            matched_signals: vec![],
        };
        let plain = make_server("bounty-viewer", "Read bounties and rewards");
        let idx = build(vec![bounty, plain]);

        let hits = idx.search(
            "bounties rewards",
            &SearchFilters {
                category: Some("bounty"),
                currency: Some("usdc"),
                ..Default::default()
            },
            10,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "bounty-board");
    }

    #[test]
    fn empty_query_browses_by_prior_with_filters() {
        let mut popular = make_server("popular-tool", "General agent tooling");
        popular.source_count = 4;
        let idx = build(vec![
            make_server("plain-tool", "General agent tooling"),
            popular,
        ]);
        let hits = idx.search("", &SearchFilters::default(), 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].name, "popular-tool", "browse mode orders by prior");
    }

    #[test]
    fn quality_prior_is_bounded() {
        let mut maxed = make_server("maxed", "x");
        maxed.source_count = 100;
        maxed.upstream_quality_score = Some(1.0);
        maxed.layer3_analysis = Some(Layer3Analysis {
            extracted_addresses: vec![],
            npm_weekly_downloads: Some(u64::MAX / 2),
            github_stars: Some(u32::MAX),
            readme_excerpt: None,
            probed: true,
            probed_at: chrono::Utc::now(),
        });
        let q = quality_prior(&maxed);
        assert!((0.1..=2.0).contains(&q));
        let plain = make_server("plain", "x");
        assert!((quality_prior(&plain) - 1.104).abs() < 0.01); // 1 + 0.15*ln(2)

        // Degenerate repetitive text sinks below neutral (anti-stuffing).
        let stuffed = make_server(
            "stuffed",
            "swap swap swap swap swap token token swap swap swap swap swap swap",
        );
        assert!(
            quality_prior(&stuffed) < 0.75,
            "stuffed text must be penalized, got {}",
            quality_prior(&stuffed)
        );
    }
}
