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
    /// When the settlement landed on chain. `None` when the RPC omitted
    /// `blockTime` for the signature — recorded honestly as unknown rather
    /// than fabricated as "now", which would corrupt any future time-decay
    /// weighting over the graph.
    #[serde(default)]
    pub settled_at: Option<chrono::DateTime<chrono::Utc>>,
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
    /// Rank-normalized score in [0, 1]: `1 - (rank-1)/n`. Rank 1 → 1.0.
    /// This is the value `composite_trust` consumes — raw EigenTrust
    /// scores sum to 1.0 across the whole graph, so their magnitude is
    /// graph-size-dependent and meaningless as a per-agent 0..1 signal.
    pub rank_normalized: f64,
    /// Settlements where this wallet was paid (incoming edges).
    pub settlements_received: u32,
    /// Settlements where this wallet paid (outgoing edges).
    pub settlements_paid: u32,
    /// Distinct counterparties across both directions — the Sybil-relevant
    /// diversity signal.
    pub counterparty_count: u32,
    /// When THIS REBUILD ran — every agent gets the same stamp on every
    /// rebuild, even when no new edges arrived. Pair with
    /// `newest_edge_settled_at` to tell "recomputed" from "actually new".
    pub computed_at: chrono::DateTime<chrono::Utc>,
    /// The most recent on-chain settlement touching this wallet (either
    /// direction). `None` when the wallet has no edge with a known
    /// block time. This is the staleness signal `computed_at` is not.
    #[serde(default)]
    pub newest_edge_settled_at: Option<chrono::DateTime<chrono::Utc>>,
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
    let mut newest_edge: HashMap<&str, chrono::DateTime<chrono::Utc>> = HashMap::new();
    for d in &unique {
        if let Some(at) = d.settled_at {
            for wallet in [d.from.as_str(), d.to.as_str()] {
                let entry = newest_edge.entry(wallet).or_insert(at);
                if at > *entry {
                    *entry = at;
                }
            }
        }
        // saturating: the rank computed below already uses checked arithmetic,
        // and a u32 overflow here would silently wrap a settlement count.
        let r = received.entry(d.to.as_str()).or_default();
        *r = r.saturating_add(1);
        let p = paid.entry(d.from.as_str()).or_default();
        *p = p.saturating_add(1);
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

    let n = scored.len().max(1) as f64;
    let agents: Vec<AgentReputation> = scored
        .into_iter()
        .enumerate()
        .map(|(i, (wallet, score))| AgentReputation {
            rank_normalized: 1.0 - (i as f64) / n,
            settlements_received: received.get(wallet.as_str()).copied().unwrap_or(0),
            settlements_paid: paid.get(wallet.as_str()).copied().unwrap_or(0),
            counterparty_count: counterparties
                .get(wallet.as_str())
                .map(|s| s.len() as u32)
                .unwrap_or(0),
            rank: (i as u32).saturating_add(1),
            eigentrust_score: score,
            newest_edge_settled_at: newest_edge.get(wallet.as_str()).copied(),
            wallet,
            computed_at: now,
        })
        .collect();

    // Postcondition: ranks are 1..=n and scores are rank-ordered. This is the
    // ONLY invariant check in build_reputation, and as a debug_assert! it was
    // compiled out of the release builds that actually serve reputation — so it
    // never ran where it mattered. A violated ordering here means the public
    // leaderboard is wrong, which is worth failing on.
    assert!(
        agents
            .windows(2)
            .all(|w| w[0].eigentrust_score >= w[1].eigentrust_score && w[0].rank < w[1].rank),
        "reputation ranks must be strictly increasing as scores decrease"
    );

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
            settled_at: Some(chrono::Utc::now()),
        }
    }

    fn pre(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn newest_edge_settled_at_is_max_over_both_directions_and_null_without_dates() {
        let t1 = chrono::DateTime::from_timestamp(1_000, 0).expect("valid ts");
        let t2 = chrono::DateTime::from_timestamp(2_000, 0).expect("valid ts");
        let mut e1 = edge("s1", "root", "a", 1.0);
        e1.settled_at = Some(t1);
        let mut e2 = edge("s2", "a", "b", 1.0);
        e2.settled_at = Some(t2);
        // An edge whose RPC read had no blockTime: recorded, but dateless.
        let mut e3 = edge("s3", "root", "c", 1.0);
        e3.settled_at = None;

        let build = build_reputation(
            &[e1, e2, e3],
            &pre(&["root"]),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .expect("build succeeds");
        let by = |w: &str| {
            build
                .agents
                .iter()
                .find(|r| r.wallet == w)
                .unwrap_or_else(|| panic!("{w} scored"))
        };
        // `a` received s1 (t1) and paid s2 (t2) — the PAID edge is newer and
        // still counts: staleness is about graph activity, not income only.
        assert_eq!(by("a").newest_edge_settled_at, Some(t2));
        assert_eq!(by("b").newest_edge_settled_at, Some(t2));
        // `c`'s only edge is dateless — honest null, not a fabricated now.
        assert_eq!(by("c").newest_edge_settled_at, None);
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
        assert!(a.rank_normalized > 0.0 && a.rank_normalized <= 1.0);
        assert_eq!(build.agents[0].rank_normalized, 1.0); // rank 1 → 1.0
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

    // ---- combinatorial graph-shape matrix -------------------------------
    //
    // Every (topology × anchor-set × weight-level) point must uphold the
    // build invariants: ranks are a 1..=n permutation, rank_normalized
    // follows its formula, settlement counters match the deduped edge
    // multiset, and the build converges.

    fn topology(name: &str, w: f64) -> Vec<TrustEdgeDoc> {
        let e = |tx: &str, from: &str, to: &str| edge(tx, from, to, w);
        match name {
            "single" => vec![e("t1", "anchor", "a")],
            "chain" => vec![e("t1", "anchor", "a"), e("t2", "a", "b"), e("t3", "b", "c")],
            "star_out" => vec![
                e("t1", "anchor", "a"),
                e("t2", "anchor", "b"),
                e("t3", "anchor", "c"),
            ],
            "star_in" => vec![
                e("t1", "anchor", "hub"),
                e("t2", "a", "hub"),
                e("t3", "b", "hub"),
            ],
            "diamond" => vec![
                e("t1", "anchor", "a"),
                e("t2", "anchor", "b"),
                e("t3", "a", "c"),
                e("t4", "b", "c"),
            ],
            "ring_detached" => vec![
                e("t1", "anchor", "honest"),
                e("s1", "x", "y"),
                e("s2", "y", "x"),
            ],
            "dup_sigs" => vec![
                e("t1", "anchor", "a"),
                e("t1", "anchor", "a"),
                e("t2", "a", "b"),
            ],
            other => panic!("unknown topology {other}"),
        }
    }

    #[test]
    fn graph_shape_matrix_upholds_build_invariants() {
        let topologies = [
            "single",
            "chain",
            "star_out",
            "star_in",
            "diamond",
            "ring_detached",
            "dup_sigs",
        ];
        let anchor_sets: [&[&str]; 3] = [
            &["anchor"],
            &["anchor", "a"],
            &["not_in_graph"], // anchor never appears in any edge
        ];
        for topo in topologies {
            for anchors in anchor_sets {
                for weight in [0.0, 0.5, 1.0] {
                    let docs = topology(topo, weight);
                    let build = build_reputation(
                        &docs,
                        &pre(anchors),
                        &EigenTrustConfig::default(),
                        chrono::Utc::now(),
                    )
                    .unwrap_or_else(|e| panic!("{topo}/{anchors:?}/{weight}: {e}"));
                    let label = format!("{topo}/{anchors:?}/{weight}");

                    // Ranks are a 1..=n permutation.
                    let mut ranks: Vec<u32> = build.agents.iter().map(|a| a.rank).collect();
                    ranks.sort_unstable();
                    assert_eq!(
                        ranks,
                        (1..=build.agents.len() as u32).collect::<Vec<_>>(),
                        "{label}: ranks are not a permutation"
                    );

                    let n = build.agents.len() as f64;
                    let mut unique_sigs: std::collections::HashSet<&str> =
                        std::collections::HashSet::new();
                    for d in &docs {
                        unique_sigs.insert(&d.tx_sig);
                    }
                    assert_eq!(
                        build.edge_count,
                        unique_sigs.len(),
                        "{label}: edge count != unique tx sigs"
                    );
                    assert_eq!(
                        build.duplicate_edges_dropped,
                        docs.len() - unique_sigs.len(),
                        "{label}: dedup count wrong"
                    );

                    for a in &build.agents {
                        let expected_norm = 1.0 - ((a.rank as f64 - 1.0) / n.max(1.0));
                        assert!(
                            (a.rank_normalized - expected_norm).abs() < 1e-9,
                            "{label}: rank_normalized formula violated for {}",
                            a.wallet
                        );
                        // Settlement counters replay the deduped multiset.
                        let mut seen: std::collections::HashSet<&str> =
                            std::collections::HashSet::new();
                        let (mut recv, mut paid) = (0u32, 0u32);
                        for d in &docs {
                            if !seen.insert(&d.tx_sig) {
                                continue;
                            }
                            if d.to == a.wallet {
                                recv += 1;
                            }
                            if d.from == a.wallet {
                                paid += 1;
                            }
                        }
                        assert_eq!(
                            (a.settlements_received, a.settlements_paid),
                            (recv, paid),
                            "{label}: settlement counters wrong for {}",
                            a.wallet
                        );
                    }
                    assert!(build.converged, "{label}: did not converge");
                }
            }
        }
    }

    #[test]
    fn anchored_agents_outrank_detached_across_weights_and_anchor_sets() {
        for weight in [0.5, 1.0] {
            let docs = topology("ring_detached", weight);
            let build = build_reputation(
                &docs,
                &pre(&["anchor"]),
                &EigenTrustConfig::default(),
                chrono::Utc::now(),
            )
            .expect("build");
            let rank_of = |w: &str| {
                build
                    .agents
                    .iter()
                    .find(|a| a.wallet == w)
                    .map(|a| a.rank)
                    .unwrap_or(u32::MAX)
            };
            assert!(
                rank_of("honest") < rank_of("x") && rank_of("honest") < rank_of("y"),
                "weight {weight}: detached ring outranked the anchored agent"
            );
        }
    }
}
