#![deny(warnings)]
#![deny(clippy::all)]

//! Reputation indexer — the pure assembly layer between settlement edges
//! and EigenTrust scores.
//!
//! This is the "caller" the `eigentrust` crate's docs describe: it builds
//! the edge list and decides what the scores mean. Like `eigentrust`, it
//! is **pure — no I/O, no Firestore, no RPC**. The mcp-server handler owns
//! reading `trust_edges` docs and writing `agent_reputation` docs; this
//! crate owns the transform in between:
//!
//! ```text
//! Vec<TrustEdgeDoc>  (Firestore `trust_edges/{tx_sig}` — settlement facts)
//!        │  edges are captured at settlement time by the finalize path
//!        │  (event-driven) + a one-time archival backfill; Task PDAs are
//!        │  closed on settlement so the edge store IS the durable record
//!        ▼
//! build_reputation(docs, pre_trusted, config)
//!        │  dedupe by tx_sig → eigentrust::TrustEdge → compute_eigentrust
//!        ▼
//! Vec<AgentReputation>  (score, rank, per-agent edge stats — one doc per
//!                        wallet that appears in the graph)
//! ```
//!
//! Weight semantics: an edge's weight is the settlement outcome quality
//! (`composite_score / score_max` at finalize, 0.0 for a lost challenge).
//! Node identity is the wallet string as recorded by the settlement
//! (base58 Solana today; CAIP-10 when EVM legs go mainnet — the type is
//! deliberately an opaque string, matching `eigentrust::PeerId`).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub use eigentrust::{EigenTrustConfig, EigenTrustError};

/// One settlement edge as stored in Firestore `trust_edges/{tx_sig}`.
/// Doc id = the settlement transaction signature (idempotent by
/// construction: re-capturing the same settlement overwrites the same doc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEdgeDoc {
    /// Settlement tx signature — the doc id, repeated in the body so the
    /// indexer can dedupe defensively.
    pub tx_sig: String,
    /// Paying counterparty (client wallet).
    pub from: String,
    /// Paid worker (agent wallet).
    pub to: String,
    /// Outcome quality in [0, 1]: composite_score / score_max at
    /// finalize; 0.0 for a settlement the agent lost (challenge).
    pub weight: f64,
    /// On-chain task id the settlement belongs to.
    pub task_id: u64,
    /// "shillbot_finalize" | "challenge_resolved".
    pub source: String,
    /// CAIP-2 chain id the settlement happened on (e.g.
    /// "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp").
    pub chain: String,
    pub settled_at: chrono::DateTime<chrono::Utc>,
}

/// Computed reputation for one wallet — what the mcp-server writes to
/// Firestore `agent_reputation/{wallet}` and `composite_trust` consumes
/// as its EigenTrust input signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReputation {
    pub wallet: String,
    /// Global EigenTrust score (all scores sum to ~1.0 across the graph).
    pub eigentrust_score: f64,
    /// 1-based rank by score (1 = most trusted).
    pub rank: u32,
    /// Settlements where this wallet was paid (incoming edges).
    pub settlements_received: u32,
    /// Settlements where this wallet paid (outgoing edges).
    pub settlements_paid: u32,
    /// Distinct counterparties across both directions — the Sybil-relevant
    /// diversity signal.
    pub counterparty_count: u32,
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

/// Result of one reputation build.
#[derive(Debug, Clone, Serialize)]
pub struct ReputationBuild {
    pub agents: Vec<AgentReputation>,
    pub edge_count: usize,
    pub duplicate_edges_dropped: usize,
    pub converged: bool,
    pub iterations: usize,
}

/// Build per-agent reputation from settlement edge docs.
///
/// - `docs`: the full `trust_edges` collection (small: one doc per
///   settlement; dedupe by `tx_sig` is defensive — the doc id already
///   guarantees uniqueness at the store level).
/// - `pre_trusted`: anchor wallets (first-party operated). Trust
///   originates only here (EigenTrust α-blend); an empty set is an error
///   (`EigenTrustError::NoPreTrustedPeers`).
pub fn build_reputation(
    docs: &[TrustEdgeDoc],
    pre_trusted: &HashSet<String>,
    config: &EigenTrustConfig,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<ReputationBuild, EigenTrustError> {
    // Dedupe by tx_sig (defensive; the Firestore doc id is the tx_sig).
    let mut seen: HashSet<&str> = HashSet::with_capacity(docs.len());
    let mut unique: Vec<&TrustEdgeDoc> = Vec::with_capacity(docs.len());
    for d in docs {
        if seen.insert(d.tx_sig.as_str()) {
            unique.push(d);
        }
    }
    let duplicate_edges_dropped = docs.len().saturating_sub(unique.len());

    let edges: Vec<eigentrust::TrustEdge> = unique
        .iter()
        .map(|d| eigentrust::TrustEdge {
            from: d.from.clone(),
            to: d.to.clone(),
            weight: d.weight.clamp(0.0, 1.0),
        })
        .collect();

    let result = eigentrust::compute_eigentrust(&edges, pre_trusted, config)?;

    // Per-agent edge stats.
    let mut received: HashMap<&str, u32> = HashMap::new();
    let mut paid: HashMap<&str, u32> = HashMap::new();
    let mut counterparties: HashMap<&str, HashSet<&str>> = HashMap::new();
    for d in &unique {
        *received.entry(d.to.as_str()).or_default() += 1;
        *paid.entry(d.from.as_str()).or_default() += 1;
        counterparties
            .entry(d.to.as_str())
            .or_default()
            .insert(d.from.as_str());
        counterparties
            .entry(d.from.as_str())
            .or_default()
            .insert(d.to.as_str());
    }

    // Rank by score, descending; ties broken by wallet for determinism.
    let mut scored: Vec<(String, f64)> = result.scores.into_iter().collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let agents: Vec<AgentReputation> = scored
        .into_iter()
        .enumerate()
        .map(|(i, (wallet, score))| AgentReputation {
            settlements_received: received.get(wallet.as_str()).copied().unwrap_or(0),
            settlements_paid: paid.get(wallet.as_str()).copied().unwrap_or(0),
            counterparty_count: counterparties
                .get(wallet.as_str())
                .map(|s| s.len() as u32)
                .unwrap_or(0),
            rank: (i as u32).saturating_add(1),
            eigentrust_score: score,
            wallet,
            computed_at: now,
        })
        .collect();

    // Postconditions: ranks are 1..=n and scores are rank-ordered.
    debug_assert!(agents
        .windows(2)
        .all(|w| { w[0].eigentrust_score >= w[1].eigentrust_score && w[0].rank < w[1].rank }));

    Ok(ReputationBuild {
        edge_count: unique.len(),
        duplicate_edges_dropped,
        converged: result.converged,
        iterations: result.iterations,
        agents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(tx: &str, from: &str, to: &str, weight: f64) -> TrustEdgeDoc {
        TrustEdgeDoc {
            tx_sig: tx.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            weight,
            task_id: 1,
            source: "shillbot_finalize".to_string(),
            chain: "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp".to_string(),
            settled_at: chrono::Utc::now(),
        }
    }

    fn pre(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn settled_agent_outranks_sybil_cluster() {
        // Anchor client pays a real agent twice (good scores); a Sybil
        // cluster vouches for itself with high weights but no anchor edge
        // ever reaches it → real agent must outrank every Sybil.
        let docs = vec![
            edge("t1", "anchor_client", "real_agent", 0.9),
            edge("t2", "anchor_client", "real_agent", 0.8),
            edge("s1", "sybil_a", "sybil_b", 1.0),
            edge("s2", "sybil_b", "sybil_c", 1.0),
            edge("s3", "sybil_c", "sybil_a", 1.0),
        ];
        let build = build_reputation(
            &docs,
            &pre(&["anchor_client"]),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .expect("build");
        let get = |w: &str| {
            build
                .agents
                .iter()
                .find(|a| a.wallet == w)
                .expect("agent present")
        };
        let real = get("real_agent");
        for sybil in ["sybil_a", "sybil_b", "sybil_c"] {
            assert!(
                real.eigentrust_score > get(sybil).eigentrust_score,
                "real agent must outrank {sybil}"
            );
        }
        assert!(build.converged);
    }

    #[test]
    fn duplicate_tx_sigs_are_dropped() {
        let docs = vec![
            edge("t1", "anchor", "agent", 0.9),
            edge("t1", "anchor", "agent", 0.9), // replayed capture
        ];
        let build = build_reputation(
            &docs,
            &pre(&["anchor"]),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .expect("build");
        assert_eq!(build.edge_count, 1);
        assert_eq!(build.duplicate_edges_dropped, 1);
        let agent = build.agents.iter().find(|a| a.wallet == "agent").unwrap();
        assert_eq!(agent.settlements_received, 1);
    }

    #[test]
    fn edge_stats_and_ranks_are_correct() {
        let docs = vec![
            edge("t1", "anchor", "agent_a", 0.9),
            edge("t2", "anchor", "agent_a", 0.9),
            edge("t3", "anchor", "agent_b", 0.5),
            edge("t4", "other_client", "agent_a", 0.7),
        ];
        let build = build_reputation(
            &docs,
            &pre(&["anchor"]),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .expect("build");
        let a = build.agents.iter().find(|x| x.wallet == "agent_a").unwrap();
        assert_eq!(a.settlements_received, 3);
        assert_eq!(a.settlements_paid, 0);
        assert_eq!(a.counterparty_count, 2); // anchor + other_client
                                             // Ranks are a permutation of 1..=n.
        let mut ranks: Vec<u32> = build.agents.iter().map(|x| x.rank).collect();
        ranks.sort_unstable();
        assert_eq!(ranks, (1..=build.agents.len() as u32).collect::<Vec<_>>());
        // agent_a (more anchored settled volume) outranks agent_b.
        let b = build.agents.iter().find(|x| x.wallet == "agent_b").unwrap();
        assert!(a.eigentrust_score > b.eigentrust_score);
    }

    #[test]
    fn empty_pre_trusted_is_an_error() {
        let docs = vec![edge("t1", "c", "a", 0.9)];
        let err = build_reputation(
            &docs,
            &HashSet::new(),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .unwrap_err();
        assert_eq!(err, EigenTrustError::NoPreTrustedPeers);
    }

    #[test]
    fn lost_challenge_edge_carries_zero_weight_without_error() {
        let docs = vec![
            edge("t1", "anchor", "good_agent", 0.9),
            edge("t2", "anchor", "bad_agent", 0.0), // lost challenge
        ];
        let build = build_reputation(
            &docs,
            &pre(&["anchor"]),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .expect("zero-weight edges must not break the build");
        let good = build
            .agents
            .iter()
            .find(|a| a.wallet == "good_agent")
            .unwrap();
        let bad = build.agents.iter().find(|a| a.wallet == "bad_agent");
        // bad_agent either has no score entry or scores strictly lower.
        if let Some(bad) = bad {
            assert!(good.eigentrust_score > bad.eigentrust_score);
        }
    }
}
