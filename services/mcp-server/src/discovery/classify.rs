//! Layer 1 classification: pure pattern matching, no LLM, runs on every server.
//!
//! Tuned for HIGH RECALL on `cash_flow_direction = EarnsForAgent` and
//! `value_to_swarm = AggregateListing` per the user's locked priority order:
//! "mostly earning opportunities, composable primites, market intelligence...
//! in that order." A false positive (we wrongly flag something as an earning
//! opp) costs a 10-second human review. A false negative (we miss a real
//! earning opp) costs us a missing entry on swarm.tips forever — that's
//! the worst outcome.
//!
//! Layer 2 (LLM) handles the ambiguous remainder. Layer 1 only fires when
//! it's at least somewhat confident.

use crate::discovery::models::{
    CashFlowDirection, Category, Layer1Classification, RawServer, ValueToSwarm,
};

/// Run all Layer 1 heuristics against a raw server.
pub fn classify_layer1(raw: &RawServer) -> Layer1Classification {
    let haystack = build_haystack(raw);

    let mut category = None;
    let mut cash_flow = None;
    let mut value = None;
    let mut currencies: Vec<String> = Vec::new();
    let mut signals: Vec<String> = Vec::new();

    // Earning signals (TIER 1 PRIORITY — be loose, prefer false positives)
    if matches_any(&haystack, EARNING_KEYWORDS) {
        cash_flow = Some(CashFlowDirection::EarnsForAgent);
        value = Some(ValueToSwarm::AggregateListing);
        signals.push("earning_keyword".to_string());
    }

    // Bounty / marketplace category (overlaps with earning but more specific)
    if matches_any(&haystack, BOUNTY_KEYWORDS) {
        category = Some(Category::Bounty);
        // Bounty servers almost always involve earning
        if cash_flow.is_none() {
            cash_flow = Some(CashFlowDirection::EarnsForAgent);
            value = Some(ValueToSwarm::AggregateListing);
        }
        signals.push("bounty_keyword".to_string());
    }

    // Payment / tip / x402 protocol — these are primitives we care about (tier 2)
    if matches_any(&haystack, PAYMENT_KEYWORDS) {
        if category.is_none() {
            category = Some(Category::Payment);
        }
        if value.is_none() {
            value = Some(ValueToSwarm::Dependency);
        }
        signals.push("payment_keyword".to_string());
    }

    // Game category — mostly competitive intelligence (tier 3)
    if matches_any(&haystack, GAME_KEYWORDS) {
        if category.is_none() {
            category = Some(Category::Game);
        }
        if value.is_none() {
            value = Some(ValueToSwarm::Competitor);
        }
        signals.push("game_keyword".to_string());
    }

    // Infrastructure (RPC, indexer, oracle) — tier 2 dependencies
    if matches_any(&haystack, INFRA_KEYWORDS) {
        if category.is_none() {
            category = Some(Category::Infrastructure);
        }
        if value.is_none() {
            value = Some(ValueToSwarm::Dependency);
        }
        signals.push("infra_keyword".to_string());
    }

    // Currency detection — word-boundary checks so "sol" doesn't match "solver"
    if contains_word(&haystack, "solana") || contains_word(&haystack, "sol") {
        currencies.push("SOL".to_string());
        signals.push("currency_solana".to_string());
    }
    if contains_word(&haystack, "usdc") {
        currencies.push("USDC".to_string());
        signals.push("currency_usdc".to_string());
    }
    if contains_word(&haystack, "ethereum") || contains_word(&haystack, "eth") {
        currencies.push("ETH".to_string());
        signals.push("currency_eth".to_string());
    }
    if contains_word(&haystack, "base") && currencies.iter().any(|c| c == "ETH" || c == "USDC") {
        // Already implied, but flag the chain
        signals.push("chain_base".to_string());
    }
    if contains_word(&haystack, "x402") {
        if !currencies.contains(&"USDC".to_string()) {
            currencies.push("USDC".to_string());
        }
        signals.push("x402_protocol".to_string());
        // x402 is a payment protocol — bias toward Payment if not yet categorized
        if category.is_none() {
            category = Some(Category::Payment);
        }
    }

    // Confidence: any signal at all == confident enough to skip Layer 2.
    // Tier-1 (earning) signals weigh more — they short-circuit ambiguity.
    let confident = !signals.is_empty();

    Layer1Classification {
        category,
        cash_flow_direction: cash_flow,
        currencies,
        value_to_swarm: value,
        confident,
        matched_signals: signals,
    }
}

/// Build the lowercased haystack we run all keyword checks against.
fn build_haystack(raw: &RawServer) -> String {
    let mut s = String::new();
    s.push_str(&raw.name.to_lowercase());
    s.push(' ');
    if let Some(t) = &raw.title {
        s.push_str(&t.to_lowercase());
        s.push(' ');
    }
    if let Some(d) = &raw.description {
        s.push_str(&d.to_lowercase());
        s.push(' ');
    }
    if let Some(repo) = &raw.github_repo {
        s.push_str(&repo.to_lowercase());
        s.push(' ');
    }
    if let Some(npm) = &raw.npm_package {
        s.push_str(&npm.to_lowercase());
        s.push(' ');
    }
    s
}

/// Word-boundary-aware match against any needle in `needles`.
///
/// CRITICAL: never use plain `String::contains` for keyword matching here.
/// Production audit (2026-04-07) found ~14 false positives in the first
/// 35 earning candidates because `EARNING_KEYWORDS` contains the bare token
/// `"earn"` and substring matching fired on "**L**earn", "L**earn**ing",
/// "Hebbian l**earn**ing", "reinforcement l**earn**ing", "Microsoft L**earn**",
/// and "**earn**ings" (data sources, not loops). Word-boundary matching kills
/// the entire family in one pass. `contains_word` already handles internal
/// spaces (e.g. "task marketplace") because it only checks the chars
/// immediately before/after the *whole* needle, not each internal token.
fn matches_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| contains_word(haystack, n))
}

/// Word-boundary-ish check — does the haystack contain `word` surrounded by
/// non-alphanumeric chars (or at start/end)? Avoids matching "sol" inside
/// "solar" or "solver".
fn contains_word(haystack: &str, word: &str) -> bool {
    let needle = word.to_lowercase();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(&needle) {
        let abs = start.saturating_add(pos);
        let before_ok = abs == 0
            || !haystack
                .as_bytes()
                .get(abs.saturating_sub(1))
                .map(|b| b.is_ascii_alphanumeric())
                .unwrap_or(false);
        let after_idx = abs.saturating_add(needle.len());
        let after_ok = after_idx >= haystack.len()
            || !haystack
                .as_bytes()
                .get(after_idx)
                .map(|b| b.is_ascii_alphanumeric())
                .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
        start = abs.saturating_add(1);
        if start >= haystack.len() {
            break;
        }
    }
    false
}

// -- Keyword lists (each one tuned for high recall on its tier) --

/// Tier 1: anything that suggests an agent could earn money by calling this.
/// Be liberal — false positives go to manual review, false negatives are missed
/// opportunities forever.
const EARNING_KEYWORDS: &[&str] = &[
    // Verb forms — explicitly enumerated because `matches_any` is now word-
    // boundary-strict on BOTH ends. Without "earning"/"earns", a description
    // like "agents earning ETH" wouldn't match because "earn" is followed by
    // an alphanumeric. We deliberately do NOT include "earnings" because
    // that token shows up in non-earning data sources (SEC filings, college
    // earnings stats) and Layer 2 LLM is the right tool to disambiguate.
    "earn",
    "earns",
    "earning",
    "payout",
    "payouts",
    "reward",
    "rewards",
    // "claim" + inflections. Risky — also matches "factual claim", "insurance
    // claim" — but most real earning servers say "claim a task / claim a
    // bounty" so the net signal is positive. Layer 2 LLM sweeps residual noise.
    "claim",
    "claims",
    "claiming",
    "submit work",
    "submit_work",
    "submit-work",
    "task marketplace",
    "agent marketplace",
    "agents earn",
    "paid task",
    "paid tasks",
    "freelance",
    "gig",
    "gigs",
    "bounty",
    "bounties",
    "escrow",
    "stipend",
    "tipping",
    "tip jar",
    "monetize",
    "monetization",
    "agent income",
    "agent earnings",
    // Game-but-earning: wagering on games where you can win money
    "wager",
    "wagers",
    "wagered",
    "wagering",
    "stake-to-play",
    "stake to play",
    "prize pool",
    "winner takes",
];

/// Bounty/marketplace specifically — strong signal for category=Bounty
const BOUNTY_KEYWORDS: &[&str] = &[
    "bounty",
    "bounties",
    "task marketplace",
    "agent marketplace",
    "gig",
    "freelance",
];

/// Payment / micropayment / tipping primitives — tier 2 dependency signal
const PAYMENT_KEYWORDS: &[&str] = &[
    "x402",
    "stripe",
    "checkout",
    "tipping",
    "tip jar",
    "micropayment",
    "micropayments",
    "payment rail",
    "payment protocol",
    "stablecoin",
    "usdc",
    "settle",
    "settlement",
];

/// Game / wagering / coordination — tier 3 competitive intel
const GAME_KEYWORDS: &[&str] = &[
    "game",
    "games",
    "wager",
    "wagers",
    "wagering",
    "stake",
    "stakes",
    "duel",
    "duels",
    "tournament",
    "tournaments",
    "social deduction",
    "leaderboard",
    "leaderboards",
];

/// Infrastructure: RPC, indexer, oracle, storage — tier 2 dependency
const INFRA_KEYWORDS: &[&str] = &[
    "rpc",
    "indexer",
    "oracle",
    "node provider",
    "storage",
    "filecoin",
    "arweave",
    "ipfs",
    "graphql index",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_with_desc(name: &str, desc: &str) -> RawServer {
        RawServer {
            name: name.to_string(),
            title: None,
            description: Some(desc.to_string()),
            endpoint: None,
            transport: None,
            npm_package: None,
            github_repo: None,
            source: "test".to_string(),
            upstream_quality_score: None,
            upstream_visitors_estimate: None,
        }
    }

    #[test]
    fn classifies_obvious_bounty_server_as_earning() {
        let r = raw_with_desc(
            "io.github.example/bounty-board",
            "MCP server for browsing and claiming open-source bounties",
        );
        let c = classify_layer1(&r);
        assert!(c.confident);
        assert_eq!(
            c.cash_flow_direction,
            Some(CashFlowDirection::EarnsForAgent)
        );
        assert_eq!(c.value_to_swarm, Some(ValueToSwarm::AggregateListing));
        assert_eq!(c.category, Some(Category::Bounty));
    }

    #[test]
    fn classifies_freelance_marketplace_as_earning() {
        let r = raw_with_desc(
            "io.github.example/agent-gigs",
            "Freelance gig marketplace where AI agents earn ETH on Base",
        );
        let c = classify_layer1(&r);
        assert_eq!(
            c.cash_flow_direction,
            Some(CashFlowDirection::EarnsForAgent)
        );
        assert!(c.currencies.contains(&"ETH".to_string()));
    }

    #[test]
    fn classifies_x402_payment_server_as_dependency() {
        let r = raw_with_desc(
            "io.github.example/x402-checkout",
            "x402 payment server for paid API calls in USDC",
        );
        let c = classify_layer1(&r);
        assert_eq!(c.category, Some(Category::Payment));
        assert!(c.currencies.contains(&"USDC".to_string()));
        assert!(c.matched_signals.contains(&"x402_protocol".to_string()));
    }

    #[test]
    fn classifies_solana_rpc_as_infrastructure_dependency() {
        let r = raw_with_desc(
            "io.github.example/solana-rpc",
            "Solana RPC node provider for autonomous agents",
        );
        let c = classify_layer1(&r);
        assert_eq!(c.category, Some(Category::Infrastructure));
        assert_eq!(c.value_to_swarm, Some(ValueToSwarm::Dependency));
        assert!(c.currencies.contains(&"SOL".to_string()));
    }

    #[test]
    fn classifies_wagering_game_as_earning_first() {
        // A game with on-chain wagers is BOTH a competitor (we run a game)
        // AND an earning opportunity (you can win money). The earning
        // classification wins because tier 1 priority is locked: missing an
        // earning opp is the worst error. The category is still Game so we
        // can also surface it as competitive intel.
        let r = raw_with_desc(
            "io.github.example/duel-game",
            "Anonymous social deduction game with on-chain wagers and a leaderboard",
        );
        let c = classify_layer1(&r);
        assert_eq!(c.category, Some(Category::Game));
        assert_eq!(
            c.cash_flow_direction,
            Some(CashFlowDirection::EarnsForAgent),
            "wagering games must classify as earning opps (tier 1 priority)"
        );
        assert_eq!(c.value_to_swarm, Some(ValueToSwarm::AggregateListing));
    }

    #[test]
    fn classifies_pure_chess_game_as_competitor() {
        // A game with NO money/wager keywords stays as competitor only.
        let r = raw_with_desc(
            "io.github.example/chess-mcp",
            "Play chess against other AI agents in a tournament with leaderboard rankings",
        );
        let c = classify_layer1(&r);
        assert_eq!(c.category, Some(Category::Game));
        assert_eq!(c.value_to_swarm, Some(ValueToSwarm::Competitor));
        assert_eq!(c.cash_flow_direction, None);
    }

    #[test]
    fn unrelated_server_is_not_confident() {
        let r = raw_with_desc(
            "io.github.example/file-system",
            "Filesystem operations server for reading local directories",
        );
        let c = classify_layer1(&r);
        assert!(!c.confident, "should defer to Layer 2");
        assert_eq!(c.cash_flow_direction, None);
    }

    // Regression: production audit on 2026-04-07 found 14/35 earning candidates
    // were "Learn"/"Learning" substring matches on the bare token "earn".
    // After switching `matches_any` to word-boundary matching, none of these
    // should classify as earning candidates anymore. Each fixture below is a
    // real (paraphrased) server description that produced a false positive.
    #[test]
    fn no_false_positive_on_learn_substring() {
        let cases: &[(&str, &str)] = &[
            (
                "com.microsoft/microsoft-learn-mcp",
                "Official Microsoft Learn MCP Server – real-time, trusted docs and code samples for AI and LLMs.",
            ),
            (
                "app.aspirelearning/mcp",
                "Aspire Learning MCP — browse courses, chapters, lessons, and import LaTeX quizzes.",
            ),
            (
                "io.github.example/content-repurposer",
                "Repurpose content into Twitter threads, LinkedIn posts, Substack notes, email, and video. (Geek-Learns project)",
            ),
            (
                "io.github.example/qiskit-gym",
                "MCP server for qiskit-gym reinforcement learning quantum circuit synthesis.",
            ),
            (
                "io.github.example/hebbian-mind",
                "Associative memory via Hebbian learning. Connections strengthen through use.",
            ),
            (
                "io.github.example/cuba-memorys",
                "Persistent memory for AI agents. Knowledge graph, Hebbian learning, RRF search.",
            ),
            (
                "ai.smithery/hello-world-test",
                "Kickstart your setup with ready-to-run greetings. Learn the inten and origin of Hello World.",
            ),
            (
                "com.close/close-mcp",
                "Close CRM to manage your sales pipeline. Learn more at https://close.com",
            ),
        ];
        for (name, desc) in cases {
            let r = raw_with_desc(name, desc);
            let c = classify_layer1(&r);
            assert!(
                !matches!(
                    c.cash_flow_direction,
                    Some(CashFlowDirection::EarnsForAgent)
                ),
                "{name} should NOT be flagged as earning — desc: {desc}"
            );
            assert!(
                !matches!(c.value_to_swarm, Some(ValueToSwarm::AggregateListing)),
                "{name} should NOT be value=AggregateListing — desc: {desc}"
            );
        }
    }

    // Regression: "earnings" as a data-source noun (SEC filings, college
    // earnings stats) should not get classified as an earning loop. The fix
    // is word-boundary matching — "earn" no longer matches "earnings".
    #[test]
    fn no_false_positive_on_earnings_data_sources() {
        let cases: &[(&str, &str)] = &[
            (
                "com.earningsfeed/mcp",
                "SEC filings and insider trades in real-time. 10-K, 10-Q, 8-K, Form 4, and company lookup.",
            ),
            (
                "com.olyport/college-scorecard",
                "Higher education data: tuition, graduation rates, and earnings.",
            ),
        ];
        for (name, desc) in cases {
            let r = raw_with_desc(name, desc);
            let c = classify_layer1(&r);
            assert!(
                !matches!(
                    c.cash_flow_direction,
                    Some(CashFlowDirection::EarnsForAgent)
                ),
                "{name} should NOT be flagged as earning — desc: {desc}"
            );
        }
    }

    // Regression: "freelance" should match "freelance gig marketplace" but
    // NOT "German freelancers". Word-boundary matching draws the line at
    // the trailing alphanumeric.
    #[test]
    fn freelance_word_boundary_distinguishes_freelancers() {
        // Real earning loop — should still match.
        let r1 = raw_with_desc(
            "io.github.example/freelance-marketplace",
            "Open freelance gig marketplace for autonomous agents.",
        );
        let c1 = classify_layer1(&r1);
        assert!(
            matches!(
                c1.cash_flow_direction,
                Some(CashFlowDirection::EarnsForAgent)
            ),
            "freelance gig marketplace must still classify as earning"
        );

        // Tool for human freelancers — should NOT match.
        let r2 = raw_with_desc(
            "finance.norman/mcp-server",
            "AI-powered bookkeeping, invoicing, and VAT filing for German freelancers.",
        );
        let c2 = classify_layer1(&r2);
        assert!(
            !matches!(
                c2.cash_flow_direction,
                Some(CashFlowDirection::EarnsForAgent)
            ),
            "bookkeeping tool for human freelancers must NOT classify as earning"
        );
    }

    #[test]
    fn contains_word_does_not_match_substring() {
        // "sol" should not match "solver" or "solar"
        assert!(!contains_word("a solver project", "sol"));
        assert!(!contains_word("solar power", "sol"));
        // But a real standalone "sol" mention should match
        assert!(contains_word("paying out sol on chain", "sol"));
    }

    #[test]
    fn contains_word_matches_at_start_and_end() {
        assert!(contains_word("solana network", "solana"));
        assert!(contains_word("running on solana", "solana"));
    }

    // Recall spot-check: a corpus of 10 obvious earning servers must all
    // come back classified as earning candidates. This is the tier-1 recall
    // floor — if any of these regress, we have a hole in EARNING_KEYWORDS.
    #[test]
    fn recall_spot_check_known_earning_servers() {
        let cases = &[
            (
                "moltlaunch",
                "Agent marketplace where AI agents earn ETH for completing gigs",
            ),
            (
                "clawtasks",
                "Bounty marketplace for AI agents on Base, paid in USDC",
            ),
            ("botbounty", "ETH bounties for autonomous bots on Base L2"),
            (
                "bountycaster",
                "Farcaster bounties posted by clients, paid in stablecoins",
            ),
            (
                "coordination-game",
                "On-chain social deduction game with stake-to-play wagering",
            ),
            (
                "shillbot",
                "Content creation marketplace where agents earn SOL on Solana mainnet",
            ),
            (
                "gigs-server",
                "Freelance gigs for AI agents, paid via x402 micropayments in USDC",
            ),
            (
                "tip-jar",
                "Tipping protocol for autonomous agents to receive payouts",
            ),
            (
                "agent-tasks",
                "Paid tasks for autonomous AI agents with escrow on Base",
            ),
            (
                "rewards-mcp",
                "Claim rewards and payouts from on-chain bounty programs",
            ),
        ];

        for (name, desc) in cases {
            let r = raw_with_desc(name, desc);
            let c = classify_layer1(&r);
            assert!(
                c.confident,
                "expected {name} to be classified confidently — desc: {desc}"
            );
            assert!(
                matches!(
                    c.cash_flow_direction,
                    Some(CashFlowDirection::EarnsForAgent)
                ) || matches!(c.value_to_swarm, Some(ValueToSwarm::AggregateListing)),
                "expected {name} to be flagged as an earning candidate — got {:?} / {:?}",
                c.cash_flow_direction,
                c.value_to_swarm
            );
        }
    }
}
