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

/// Maximum weight a single trust edge may carry. Standard settlements lie in
/// `[0.0, 1.0]`; verified security bug-bounty edges (`source == "bug_bounty"`)
/// may scale up to `5.0` for critical patches.
pub const MAX_EDGE_WEIGHT: f64 = 5.0;

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
    /// Verified security/code bug bounties credited to this wallet (`source == "bug_bounty"`).
    #[serde(default)]
    pub bug_bounties_received: u32,
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
    // Dedupe by tx_sig (defensive; the Firestore doc id is the tx_sig) and
    // drop self-dealing edges (from == to) so settling a task in one's own
    // self-serve campaign cannot inflate EigenTrust or settlement counters.
    let mut seen: HashSet<&str> = HashSet::with_capacity(docs.len());
    let mut unique: Vec<&TrustEdgeDoc> = Vec::with_capacity(docs.len());
    let mut duplicate_edges_dropped = 0usize;
    for d in docs {
        if !seen.insert(d.tx_sig.as_str()) {
            duplicate_edges_dropped = duplicate_edges_dropped.saturating_add(1);
            continue;
        }
        if d.from == d.to {
            continue;
        }
        unique.push(d);
    }

    let edges: Vec<eigentrust::TrustEdge> = unique
        .iter()
        .map(|d| {
            let max_w = if d.source == "bug_bounty" {
                MAX_EDGE_WEIGHT
            } else {
                1.0
            };
            let w = if d.weight.is_finite() {
                d.weight.clamp(0.0, max_w)
            } else {
                0.0
            };
            eigentrust::TrustEdge {
                from: d.from.clone(),
                to: d.to.clone(),
                weight: w,
            }
        })
        .collect();

    let result = eigentrust::compute_eigentrust(&edges, pre_trusted, config)?;

    // Per-agent edge stats.
    let mut received: HashMap<&str, u32> = HashMap::new();
    let mut paid: HashMap<&str, u32> = HashMap::new();
    let mut bounties: HashMap<&str, u32> = HashMap::new();
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
        if d.source == "bug_bounty" {
            let b = bounties.entry(d.to.as_str()).or_default();
            *b = b.saturating_add(1);
        }
        counterparties
            .entry(d.to.as_str())
            .or_default()
            .insert(d.from.as_str());
        counterparties
            .entry(d.from.as_str())
            .or_default()
            .insert(d.to.as_str());
    }

    let mut capped_scores = result.scores;
    cap_at_entry_points(&mut capped_scores, &edges, pre_trusted, config.alpha);

    // Rank by score, descending; ties broken by wallet for determinism.
    let mut scored: Vec<(String, f64)> = capped_scores.into_iter().collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // BH-03: Detached / un-anchored wallets (`eigentrust_score <= SCORE_FLOOR`)
    // MUST receive `rank_normalized = 0.0`. Previously `1.0 - i / n` over all
    // wallets let a `"1111..."` vanity pubkey with `0.0` score lexicographically
    // jump to `rank_normalized > 0.97` via zero-score tie-breaking.
    const SCORE_FLOOR: f64 = 1e-12;
    let n_positive = scored.iter().filter(|(_, s)| *s > SCORE_FLOOR).count();
    let denom = n_positive.max(1) as f64;
    let agents: Vec<AgentReputation> = scored
        .into_iter()
        .enumerate()
        .map(|(i, (wallet, score))| AgentReputation {
            rank_normalized: if score <= SCORE_FLOOR {
                0.0
            } else {
                1.0 - (i as f64) / denom
            },
            settlements_received: received.get(wallet.as_str()).copied().unwrap_or(0),
            settlements_paid: paid.get(wallet.as_str()).copied().unwrap_or(0),
            bug_bounties_received: bounties.get(wallet.as_str()).copied().unwrap_or(0),
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

/// Cycle-safe entry-point cap (`BH-03` + `HIGH-19`):
/// Condenses non-anchor peers into strongly connected components (Tarjan SCC)
/// and propagates exact `(1.0 - alpha)` row-normalized trust across the acyclic
/// condensation DAG.
///
/// Any internal cycle — including a 2-wallet `anchor -> entry_sybil <-> sybil_2`
/// mutual-settlement loop directly adjacent to an anchor — forms a single SCC
/// whose capacity is bounded strictly by its external incoming trust from
/// `pre_trusted` anchors and strictly upstream SCCs, preventing random-walk
/// mass trapping from amplifying `entry_sybil` above its direct anchor allocation.
fn cap_at_entry_points(
    scores: &mut HashMap<String, f64>,
    edges: &[eigentrust::TrustEdge],
    pre_trusted: &HashSet<String>,
    alpha: f64,
) {
    let decay = (1.0 - alpha).clamp(0.0, 1.0);
    let mut out_sum: HashMap<&str, f64> = HashMap::new();
    let mut pair_weight: HashMap<(&str, &str), f64> = HashMap::new();
    for e in edges {
        if e.weight <= 0.0 || !e.weight.is_finite() || e.from == e.to {
            continue;
        }
        *out_sum.entry(e.from.as_str()).or_insert(0.0) += e.weight;
        *pair_weight
            .entry((e.from.as_str(), e.to.as_str()))
            .or_insert(0.0) += e.weight;
    }

    let mut non_anchors: Vec<String> = scores
        .keys()
        .filter(|w| !pre_trusted.contains(w.as_str()))
        .cloned()
        .collect();
    non_anchors.sort_unstable();
    if non_anchors.is_empty() {
        return;
    }
    let node_idx: HashMap<&str, usize> = non_anchors
        .iter()
        .enumerate()
        .map(|(i, w)| (w.as_str(), i))
        .collect();

    let n = non_anchors.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut incoming_anchors: Vec<Vec<(&str, f64)>> = vec![Vec::new(); n];
    let mut incoming_non_anchors: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];

    for (&(u, v), &w) in &pair_weight {
        let Some(&v_idx) = node_idx.get(v) else {
            // `v` is an anchor; anchors are never capped.
            continue;
        };
        if pre_trusted.contains(u) {
            incoming_anchors[v_idx].push((u, w));
        } else if let Some(&u_idx) = node_idx.get(u) {
            adj[u_idx].push(v_idx);
            incoming_non_anchors[v_idx].push((u_idx, w));
        }
    }
    for list in &mut adj {
        list.sort_unstable();
    }

    // Iterative Tarjan SCC over `0..n`. Tarjan emits SCCs in reverse topological
    // order (sinks first, sources last), so `.reverse()` gives exact source-to-sink order.
    struct TarjanState {
        index_counter: usize,
        indices: Vec<usize>,
        lowlink: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        sccs: Vec<Vec<usize>>,
    }
    fn strongconnect(v: usize, adj: &[Vec<usize>], st: &mut TarjanState) {
        st.indices[v] = st.index_counter;
        st.lowlink[v] = st.index_counter;
        st.index_counter = st.index_counter.saturating_add(1);
        st.stack.push(v);
        st.on_stack[v] = true;

        for &w in &adj[v] {
            if st.indices[w] == usize::MAX {
                strongconnect(w, adj, st);
                st.lowlink[v] = st.lowlink[v].min(st.lowlink[w]);
            } else if st.on_stack[w] {
                st.lowlink[v] = st.lowlink[v].min(st.indices[w]);
            }
        }

        if st.lowlink[v] == st.indices[v] {
            let mut comp = Vec::new();
            while let Some(w) = st.stack.pop() {
                st.on_stack[w] = false;
                comp.push(w);
                if w == v {
                    break;
                }
            }
            st.sccs.push(comp);
        }
    }

    let mut tarjan = TarjanState {
        index_counter: 0,
        indices: vec![usize::MAX; n],
        lowlink: vec![usize::MAX; n],
        on_stack: vec![false; n],
        stack: Vec::with_capacity(n),
        sccs: Vec::new(),
    };
    for v in 0..n {
        if tarjan.indices[v] == usize::MAX {
            strongconnect(v, &adj, &mut tarjan);
        }
    }
    tarjan.sccs.reverse();

    let mut comp_of = vec![0usize; n];
    for (cid, comp) in tarjan.sccs.iter().enumerate() {
        for &v in comp {
            comp_of[v] = cid;
        }
    }

    let mut caps = vec![0.0f64; n];
    for (cid, comp) in tarjan.sccs.iter().enumerate() {
        let mut external_inflow = 0.0f64;
        for &v in comp {
            for &(anchor, w) in &incoming_anchors[v] {
                let denom = out_sum.get(anchor).copied().unwrap_or(0.0);
                if denom > 0.0 {
                    let anchor_score = scores.get(anchor).copied().unwrap_or(0.0);
                    external_inflow += decay * anchor_score * (w / denom);
                }
            }
            for &(u_idx, w) in &incoming_non_anchors[v] {
                if comp_of[u_idx] != cid {
                    let u_name = non_anchors[u_idx].as_str();
                    let denom = out_sum.get(u_name).copied().unwrap_or(0.0);
                    if denom > 0.0 {
                        let u_score = scores.get(u_name).copied().unwrap_or(0.0);
                        let u_eff = caps[u_idx].min(u_score);
                        external_inflow += decay * u_eff * (w / denom);
                    }
                }
            }
        }
        for &v in comp {
            caps[v] = external_inflow;
        }
    }

    for (idx, wallet) in non_anchors.iter().enumerate() {
        if let Some(score) = scores.get_mut(wallet.as_str()) {
            if *score > caps[idx] {
                *score = caps[idx];
            }
        }
    }
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

                    let n_positive = build
                        .agents
                        .iter()
                        .filter(|a| a.eigentrust_score > 1e-12)
                        .count();
                    let denom = n_positive.max(1) as f64;
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
                        let expected_norm = if a.eigentrust_score <= 1e-12 {
                            0.0
                        } else {
                            1.0 - ((a.rank as f64 - 1.0) / denom)
                        };
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

    #[test]
    fn bug_bounty_edges_increment_counter_and_scale_up_to_max_edge_weight() {
        let mut bounty_edge = edge("bounty:commit123", "anchor", "researcher", 3.5);
        bounty_edge.source = "bug_bounty".to_string();
        let settlement_edge = edge("tx1", "anchor", "worker", 1.0);

        let build = build_reputation(
            &[bounty_edge, settlement_edge],
            &pre(&["anchor"]),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .expect("build succeeds");

        let researcher = build
            .agents
            .iter()
            .find(|a| a.wallet == "researcher")
            .expect("researcher present");
        let worker = build
            .agents
            .iter()
            .find(|a| a.wallet == "worker")
            .expect("worker present");

        assert_eq!(researcher.bug_bounties_received, 1);
        assert_eq!(researcher.settlements_received, 1);
        assert_eq!(worker.bug_bounties_received, 0);
        assert!(
            researcher.eigentrust_score > worker.eigentrust_score,
            "3.5-weight bug bounty edge must outscore 1.0-weight settlement edge from same anchor"
        );
    }

    #[test]
    fn self_dealing_edges_are_ignored() {
        let docs = vec![
            edge("tx_real", "anchor", "worker", 1.0),
            edge("tx_self_1", "worker", "worker", 1.0),
            edge("tx_self_2", "self_dealer", "self_dealer", 1.0),
        ];
        let build = build_reputation(
            &docs,
            &pre(&["anchor"]),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .expect("build succeeds");
        assert_eq!(build.edge_count, 1);
        let worker = build
            .agents
            .iter()
            .find(|a| a.wallet == "worker")
            .expect("worker present");
        assert_eq!(worker.settlements_received, 1);
        assert_eq!(worker.settlements_paid, 0);
        assert_eq!(worker.counterparty_count, 1);
        assert!(
            build.agents.iter().all(|a| a.wallet != "self_dealer"),
            "self-only dealer must not enter the reputation graph"
        );
    }

    #[test]
    fn detached_vanity_sybils_receive_zero_rank_normalized() {
        // BH-03 regression: 40 detached Sybil pairs including a `"1111..."` vanity
        // pubkey must all receive `rank_normalized == 0.0`, never `> 0.90` via
        // lexicographic zero-score tie-breaking.
        let mut docs = vec![edge("t_honest", "anchor", "z_honest", 1.0)];
        docs.push(edge(
            "s_vanity",
            "1111111111111111",
            "2222222222222222",
            1.0,
        ));
        for idx in 0..20 {
            docs.push(edge(
                &format!("s_{idx}"),
                &format!("sybil_c_{idx:02}"),
                &format!("sybil_w_{idx:02}"),
                1.0,
            ));
        }
        let build = build_reputation(
            &docs,
            &pre(&["anchor"]),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .expect("build succeeds");
        let vanity = build
            .agents
            .iter()
            .find(|a| a.wallet == "1111111111111111")
            .expect("vanity present");
        assert_eq!(vanity.eigentrust_score, 0.0);
        assert_eq!(vanity.rank_normalized, 0.0);
        for a in &build.agents {
            if a.wallet != "anchor" && a.wallet != "z_honest" {
                assert_eq!(
                    a.rank_normalized, 0.0,
                    "detached peer {} leaked rank_normalized {}",
                    a.wallet, a.rank_normalized
                );
            }
        }
    }

    #[test]
    fn path_monotonicity_caps_mutual_ring_behind_single_entry_point() {
        // BH-03 regression: `anchor` pays `honest_top` (weight 1.0) and `bridge`
        // (weight 0.1). `bridge` funnels into a mutual ring `ring_a <-> ring_b`
        // (weight 1.0). Without `cap_at_entry_points`, random-walk mass trapped
        // inside `ring_a <-> ring_b` lets the ring outrank `bridge`.
        let docs = vec![
            edge("t1", "anchor", "honest_top", 1.0),
            edge("t2", "anchor", "bridge", 0.1),
            edge("t3", "bridge", "ring_a", 1.0),
            edge("t4", "ring_a", "ring_b", 1.0),
            edge("t5", "ring_b", "ring_a", 1.0),
        ];
        let build = build_reputation(
            &docs,
            &pre(&["anchor"]),
            &EigenTrustConfig::default(),
            chrono::Utc::now(),
        )
        .expect("build succeeds");
        let score_of = |w: &str| {
            build
                .agents
                .iter()
                .find(|a| a.wallet == w)
                .expect("present")
                .eigentrust_score
        };
        let bridge_score = score_of("bridge");
        assert!(score_of("ring_a") <= bridge_score + 1e-12);
        assert!(score_of("ring_b") <= bridge_score + 1e-12);
        assert!(score_of("honest_top") > bridge_score);
    }

    #[test]
    fn direct_anchor_mutual_loop_cannot_amplify_entry_sybil() {
        // HIGH-19 regression: `anchor` pays `honest_top` (weight 0.9) and
        // `entry_sybil` (weight 0.1). `entry_sybil` and `sybil_2` settle back
        // and forth (`entry_sybil <-> sybil_2`, weight 1.0). Previously
        // `pre_trusted.contains(from)` set `caps[entry_sybil] = f64::INFINITY`,
        // letting the 2-wallet loop amplify `entry_sybil` ~5.26x and outrank
        // `honest_top`. With SCC condensation, `{entry_sybil, sybil_2}` is capped
        // at `(1 - alpha) * s(anchor) * (0.1 / 1.0)`, 9x below `honest_top`.
        let cfg = EigenTrustConfig::default();
        let docs = vec![
            edge("t1", "anchor", "honest_top", 0.9),
            edge("t2", "anchor", "entry_sybil", 0.1),
            edge("t3", "entry_sybil", "sybil_2", 1.0),
            edge("t4", "sybil_2", "entry_sybil", 1.0),
        ];
        let build =
            build_reputation(&docs, &pre(&["anchor"]), &cfg, chrono::Utc::now()).expect("build");
        let score_of = |w: &str| {
            build
                .agents
                .iter()
                .find(|a| a.wallet == w)
                .expect("present")
                .eigentrust_score
        };
        let anchor_score = score_of("anchor");
        let honest_score = score_of("honest_top");
        let entry_cap = (1.0 - cfg.alpha) * anchor_score * 0.1;
        assert!(
            score_of("entry_sybil") <= entry_cap + 1e-12,
            "entry_sybil {} exceeded direct anchor allocation {}",
            score_of("entry_sybil"),
            entry_cap
        );
        assert!(score_of("sybil_2") <= entry_cap + 1e-12);
        assert!(honest_score > score_of("entry_sybil") * 8.5);
    }
}
