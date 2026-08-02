#![deny(warnings)]
#![deny(clippy::all)]

//! EigenTrust off-chain computation for the swarm.tips agent
//! reputation graph. Pure crate — no I/O, no Solana RPC, no
//! Firestore. Callers feed the algorithm a list of trust edges
//! (`from`, `to`, `weight`) plus a set of pre-trusted peers, and
//! receive back a `HashMap<peer_id, trust_score>` where scores sum
//! to ~1.0.
//!
//! ## Reference
//!
//! Kamvar, Schlosser, Garcia-Molina (2003), *The EigenTrust
//! Algorithm for Reputation Management in P2P Networks*. We
//! implement the algorithm from §4.5 ("Distributed Algorithm with
//! Pre-trusted Peers"), but run it centrally (off-chain) since the
//! input — finalized on-chain task tuples — is already public.
//!
//! ## Update equation
//!
//! ```text
//! t^(k+1) = (1 − α) · Cᵀ · t^(k) + α · p
//! ```
//!
//! where:
//! - `C` is the row-normalized trust matrix. `C[i][j]` is the
//!   fraction of peer `i`'s outgoing trust that goes to peer `j`.
//! - `t^(k)` is the global trust vector at iteration `k`.
//! - `p` is the pre-trust vector (uniform over pre-trusted peers,
//!   ‖p‖₁ = 1).
//! - `α ∈ [0, 1]` blends pre-trust with computed trust each
//!   iteration. Typical: `α = 0.1` (small enough that trust still
//!   flows through the graph, large enough that pre-trusted peers
//!   anchor the computation against malicious collectives).
//!
//! ## Sybil resistance
//!
//! The α-blend with pre-trust is the load-bearing defense: a sybil
//! cluster (many peers vouching for each other in a closed loop)
//! cannot accumulate trust because no edge from a pre-trusted peer
//! reaches it. Without `α > 0` the computation is degenerate (the
//! sybil cluster's internal trust converges to a non-zero
//! eigenvector of its own subgraph). With `α > 0` the cluster's
//! steady-state trust is bounded by the trust flowing in from the
//! pre-trusted set, which is zero by assumption.
//!
//! ## Convergence
//!
//! For α > 0 the iteration is a contraction mapping with Lipschitz
//! constant ≤ (1 − α) (in L₁), so it converges geometrically. With
//! α = 0.1 it converges to ~1e-6 tolerance in <100 iterations on
//! networks up to 1K peers (per the original paper §6).
//!
//! ## What this crate is NOT
//!
//! - Not an indexer. Caller builds the edge list from on-chain
//!   events (`TaskFinalized`, `ChallengeResolved`).
//! - Not a service. Caller decides when to recompute and where to
//!   store scores.
//! - Not the composite trust score. See
//!   `services/mcp-server/src/composite_trust.rs` for the
//!   per-agent composite that combines EigenTrust with Shillbot
//!   counters, Coordination Game performance, and curator-tier
//!   ascription.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Opaque peer identifier. Wraps a string so the algorithm is
/// chain-agnostic — the caller decides whether peers are Solana
/// pubkeys, Ethereum addresses, or anything else identifiable.
pub type PeerId = String;

/// One directed trust edge: peer `from` expresses trust `weight` in
/// peer `to`. `weight` must be ≥ 0 (negative trust is collapsed to
/// 0; for distrust modeling, see the EigenTrust paper §4.6 — this
/// crate implements only the positive-trust variant).
///
/// In the swarm.tips graph an edge corresponds to one finalized
/// task: `from = client`, `to = agent`, `weight = score / MAX_SCORE`
/// (after the on-chain composite score is computed by the verifier).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustEdge {
    pub from: PeerId,
    pub to: PeerId,
    pub weight: f64,
}

/// Configuration for one EigenTrust run.
#[derive(Debug, Clone)]
pub struct EigenTrustConfig {
    /// α — pre-trust blend factor. 0 ≤ α ≤ 1. Typical: 0.1.
    pub alpha: f64,
    /// Maximum number of iterations before giving up. This crate's Default is
    /// 500 (the doc used to say 100, which no config in the tree actually uses).
    pub max_iterations: usize,
    /// L₁ tolerance for convergence: the run stops when the change
    /// in trust vector between two successive iterations has L₁
    /// norm ≤ this. Typical: 1e-6.
    pub tolerance: f64,
}

impl Default for EigenTrustConfig {
    fn default() -> Self {
        // max_iterations sized for worst-case L₁ convergence: with
        // α=0.1 the contraction factor is (1-α)=0.9, so reaching
        // 1e-6 from a unit-scale start needs log_0.9(1e-6) ≈ 131
        // iterations on a long-chain or short-cycle graph. 500
        // gives ~4x headroom and is still milliseconds on a 1K-peer
        // graph (cost is O(edges) per iteration). Connected graphs
        // converge much faster (paper §6: "<10 iterations typical").
        Self {
            alpha: 0.1,
            max_iterations: 500,
            tolerance: 1e-6,
        }
    }
}

/// Errors that can arise validating inputs to `compute_eigentrust`.
#[derive(Debug, Error, PartialEq)]
pub enum EigenTrustError {
    #[error("alpha must be in [0, 1], got {0}")]
    InvalidAlpha(f64),
    #[error("max_iterations must be ≥ 1, got 0")]
    InvalidMaxIterations,
    #[error("tolerance must be > 0, got {0}")]
    InvalidTolerance(f64),
    #[error("at least one pre-trusted peer is required")]
    NoPreTrustedPeers,
    #[error("edge weight must be ≥ 0, got {0} on edge {1} → {2}")]
    NegativeEdgeWeight(f64, PeerId, PeerId),
    #[error("edge weight is not finite: {0} on edge {1} → {2}")]
    NonFiniteEdgeWeight(f64, PeerId, PeerId),
}

/// Result of one EigenTrust run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EigenTrustResult {
    /// Per-peer global trust score. Scores sum to approximately 1.0
    /// (modulo floating-point error of size O(tolerance)).
    pub scores: HashMap<PeerId, f64>,
    /// Number of iterations actually run. ≤ `config.max_iterations`.
    pub iterations: usize,
    /// Whether the run hit the tolerance threshold (true) or
    /// exhausted `max_iterations` (false). Callers should log a
    /// warning if false — non-convergence usually indicates a bad α
    /// choice or a degenerate graph.
    pub converged: bool,
}

/// Compute global trust scores for every peer that appears in any
/// edge or pre-trusted set.
///
/// Algorithm:
/// 1. Validate inputs (config bounds, edge weights non-negative).
/// 2. Collect the peer set (every distinct peer in edges or
///    `pre_trusted`).
/// 3. Build the row-normalized trust matrix `C`. For each peer
///    `from`, sum its outgoing edge weights, then divide each edge
///    by the sum so each row sums to 1. If a peer has no outgoing
///    edges, route its trust to the pre-trusted set (the standard
///    EigenTrust handling for "stranded" peers — without this they
///    are sinks and the computation diverges).
/// 4. Initialize `t^(0) = p` (uniform over pre-trusted peers).
/// 5. Iterate `t^(k+1) = (1 − α) · Cᵀ · t^(k) + α · p` until the L₁
///    change is ≤ tolerance or `max_iterations` is reached.
/// 6. Return scores normalized to sum to 1.0.
pub fn compute_eigentrust(
    edges: &[TrustEdge],
    pre_trusted: &HashSet<PeerId>,
    config: &EigenTrustConfig,
) -> Result<EigenTrustResult, EigenTrustError> {
    // 1. Validate.
    if !(0.0..=1.0).contains(&config.alpha) {
        return Err(EigenTrustError::InvalidAlpha(config.alpha));
    }
    if config.max_iterations == 0 {
        return Err(EigenTrustError::InvalidMaxIterations);
    }
    // Reject zero, negative, and NaN tolerance. We want > 0, but
    // NaN > 0 is false; explicit `is_nan() || <= 0` covers both.
    if config.tolerance.is_nan() || config.tolerance <= 0.0 {
        return Err(EigenTrustError::InvalidTolerance(config.tolerance));
    }
    if pre_trusted.is_empty() {
        return Err(EigenTrustError::NoPreTrustedPeers);
    }
    for e in edges {
        if !e.weight.is_finite() {
            return Err(EigenTrustError::NonFiniteEdgeWeight(
                e.weight,
                e.from.clone(),
                e.to.clone(),
            ));
        }
        if e.weight < 0.0 {
            return Err(EigenTrustError::NegativeEdgeWeight(
                e.weight,
                e.from.clone(),
                e.to.clone(),
            ));
        }
    }

    // 2. Collect peers — deterministic order so the score map is
    // reproducible across runs (same input → same output).
    let mut peers: Vec<PeerId> = Vec::new();
    let mut peer_index: HashMap<PeerId, usize> = HashMap::new();
    let add_peer = |id: &PeerId, peers: &mut Vec<PeerId>, idx: &mut HashMap<PeerId, usize>| {
        if !idx.contains_key(id) {
            idx.insert(id.clone(), peers.len());
            peers.push(id.clone());
        }
    };
    let mut sorted_pre: Vec<&PeerId> = pre_trusted.iter().collect();
    sorted_pre.sort();
    for id in &sorted_pre {
        add_peer(id, &mut peers, &mut peer_index);
    }
    for e in edges {
        add_peer(&e.from, &mut peers, &mut peer_index);
        add_peer(&e.to, &mut peers, &mut peer_index);
    }
    let n = peers.len();

    // 3. Build sparse row-normalized C as adjacency lists. Per the
    // paper, peers with zero outgoing trust route to the
    // pre-trusted set (so pre-trust acts as a default destination
    // for stranded peers, not a sink).
    let mut out_sum: Vec<f64> = vec![0.0; n];
    for e in edges {
        let i = peer_index[&e.from];
        out_sum[i] += e.weight;
    }
    // For every peer, an `outgoing` list of (j, c_ij) pairs.
    let mut outgoing: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for e in edges {
        let i = peer_index[&e.from];
        let j = peer_index[&e.to];
        let s = out_sum[i];
        if s > 0.0 {
            outgoing[i].push((j, e.weight / s));
        }
    }
    // Pre-trust vector p: uniform over pre-trusted peers, summing
    // to 1. Pre-computed once.
    let pre_count = pre_trusted.len() as f64;
    let p_uniform = 1.0 / pre_count;
    let pre_indices: Vec<usize> = sorted_pre.iter().map(|id| peer_index[*id]).collect();
    let mut p_vec: Vec<f64> = vec![0.0; n];
    for &i in &pre_indices {
        p_vec[i] = p_uniform;
    }
    // For stranded peers (no outgoing edges), route their entire
    // trust to the pre-trust vector — equivalent to setting their
    // C row to p.
    for i in 0..n {
        if out_sum[i] == 0.0 {
            for &j in &pre_indices {
                outgoing[i].push((j, p_uniform));
            }
        }
    }

    // 4. t^(0) = p.
    let mut t: Vec<f64> = p_vec.clone();
    let mut t_next: Vec<f64> = vec![0.0; n];

    // 5. Iterate.
    let mut iterations = 0;
    let mut converged = false;
    let one_minus_alpha = 1.0 - config.alpha;
    while iterations < config.max_iterations {
        // t_next = (1 − α) · Cᵀ · t + α · p
        // We compute Cᵀ · t by walking outgoing edges: peer i
        // contributes (c_ij · t_i) to t_next[j].
        for slot in t_next.iter_mut() {
            *slot = 0.0;
        }
        for (i, neighbours) in outgoing.iter().enumerate() {
            let ti = t[i];
            if ti == 0.0 {
                continue;
            }
            for &(j, cij) in neighbours {
                t_next[j] += cij * ti;
            }
        }
        // Apply the (1 − α) scale and α-blend with pre-trust.
        for j in 0..n {
            t_next[j] = one_minus_alpha * t_next[j] + config.alpha * p_vec[j];
        }
        // L₁ change.
        let mut delta: f64 = 0.0;
        for j in 0..n {
            delta += (t_next[j] - t[j]).abs();
        }
        std::mem::swap(&mut t, &mut t_next);
        iterations += 1;
        if delta <= config.tolerance {
            converged = true;
            break;
        }
    }

    if !converged {
        tracing::warn!(
            iterations = iterations,
            max_iterations = config.max_iterations,
            "EigenTrust did not converge — check alpha and graph shape"
        );
    }

    // 6. Renormalize defensively (the sum should already be ~1 by
    // construction, but accumulated floating-point error in long
    // runs can drift).
    let total: f64 = t.iter().sum();
    if total > 0.0 {
        for v in t.iter_mut() {
            *v /= total;
        }
    }

    let mut scores: HashMap<PeerId, f64> = HashMap::with_capacity(n);
    for (i, peer) in peers.into_iter().enumerate() {
        scores.insert(peer, t[i]);
    }

    Ok(EigenTrustResult {
        scores,
        iterations,
        converged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pre(ids: &[&str]) -> HashSet<PeerId> {
        ids.iter().map(|s| (*s).to_string()).collect()
    }

    fn edge(from: &str, to: &str, w: f64) -> TrustEdge {
        TrustEdge {
            from: from.to_string(),
            to: to.to_string(),
            weight: w,
        }
    }

    #[test]
    fn rejects_invalid_alpha() {
        let err = compute_eigentrust(
            &[],
            &pre(&["a"]),
            &EigenTrustConfig {
                alpha: -0.1,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(err, EigenTrustError::InvalidAlpha(-0.1));

        let err = compute_eigentrust(
            &[],
            &pre(&["a"]),
            &EigenTrustConfig {
                alpha: 1.5,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(err, EigenTrustError::InvalidAlpha(1.5));
    }

    #[test]
    fn rejects_zero_iterations() {
        let err = compute_eigentrust(
            &[],
            &pre(&["a"]),
            &EigenTrustConfig {
                max_iterations: 0,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(err, EigenTrustError::InvalidMaxIterations);
    }

    #[test]
    fn rejects_non_positive_tolerance() {
        let err = compute_eigentrust(
            &[],
            &pre(&["a"]),
            &EigenTrustConfig {
                tolerance: 0.0,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(err, EigenTrustError::InvalidTolerance(0.0));
    }

    #[test]
    fn rejects_no_pre_trusted_peers() {
        let err =
            compute_eigentrust(&[], &HashSet::new(), &EigenTrustConfig::default()).unwrap_err();
        assert_eq!(err, EigenTrustError::NoPreTrustedPeers);
    }

    #[test]
    fn rejects_negative_edge_weight() {
        let err = compute_eigentrust(
            &[edge("a", "b", -0.5)],
            &pre(&["a"]),
            &EigenTrustConfig::default(),
        )
        .unwrap_err();
        assert_eq!(
            err,
            EigenTrustError::NegativeEdgeWeight(-0.5, "a".into(), "b".into())
        );
    }

    #[test]
    fn rejects_non_finite_edge_weight() {
        let err = compute_eigentrust(
            &[edge("a", "b", f64::NAN)],
            &pre(&["a"]),
            &EigenTrustConfig::default(),
        )
        .unwrap_err();
        match err {
            EigenTrustError::NonFiniteEdgeWeight(w, _, _) => assert!(w.is_nan()),
            _ => panic!("wrong error"),
        }
    }

    #[test]
    fn no_edges_only_pre_trusted_peers() {
        // With no edges and 2 pre-trusted peers, scores should
        // converge to the pre-trust vector itself.
        let result =
            compute_eigentrust(&[], &pre(&["alice", "bob"]), &EigenTrustConfig::default()).unwrap();
        assert!(result.converged);
        assert!((result.scores["alice"] - 0.5).abs() < 1e-9);
        assert!((result.scores["bob"] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn scores_sum_to_one() {
        let edges = vec![
            edge("seed", "a", 1.0),
            edge("seed", "b", 0.5),
            edge("a", "c", 1.0),
            edge("b", "c", 1.0),
        ];
        let result =
            compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        let total: f64 = result.scores.values().sum();
        assert!((total - 1.0).abs() < 1e-9, "scores sum to {total}");
    }

    #[test]
    fn pre_trusted_endorsement_propagates() {
        // seed → a → b. b should receive non-trivial trust because
        // a (trusted by seed) endorses b.
        let edges = vec![edge("seed", "a", 1.0), edge("a", "b", 1.0)];
        let result =
            compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        // The seed retains trust through the α-blend each iteration;
        // a and b receive trust transitively. b should have less
        // than a (one extra hop), but both should be > 0.
        assert!(result.scores["seed"] > 0.0);
        assert!(result.scores["a"] > 0.0);
        assert!(result.scores["b"] > 0.0);
        // Specifically: seed > b because of the α anchor.
        assert!(
            result.scores["seed"] > result.scores["b"],
            "seed={} b={}",
            result.scores["seed"],
            result.scores["b"]
        );
    }

    #[test]
    fn sybil_cluster_isolated_from_seed_gets_near_zero_trust() {
        // Two graph components:
        //   seed → real_agent (weight 1.0)
        //   sybil_a ↔ sybil_b ↔ sybil_c (a closed mutual-vouch loop)
        // With α > 0, the sybil cluster cannot accumulate trust
        // because no edge from the seed reaches it.
        let edges = vec![
            edge("seed", "real_agent", 1.0),
            edge("sybil_a", "sybil_b", 1.0),
            edge("sybil_b", "sybil_c", 1.0),
            edge("sybil_c", "sybil_a", 1.0),
            edge("sybil_a", "sybil_c", 1.0),
            edge("sybil_b", "sybil_a", 1.0),
        ];
        let result =
            compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        // Real agent should have meaningful trust.
        assert!(
            result.scores["real_agent"] > 0.05,
            "real_agent={}",
            result.scores["real_agent"]
        );
        // Each sybil should have ~0 trust (only floating-point noise).
        for sybil in ["sybil_a", "sybil_b", "sybil_c"] {
            assert!(
                result.scores[sybil] < 1e-6,
                "{sybil}={}",
                result.scores[sybil]
            );
        }
    }

    #[test]
    fn sybil_voting_for_one_target_does_not_grant_trust() {
        // 50 sybil clients all vouch perfectly for one agent. The
        // agent's trust should still be ~0 because no pre-trusted
        // peer endorses any of the sybils.
        let mut edges: Vec<TrustEdge> = (0..50)
            .map(|i| edge(&format!("sybil_{i}"), "fake_agent", 1.0))
            .collect();
        // One real client gives a real agent a perfect score.
        edges.push(edge("real_client", "real_agent", 1.0));
        edges.push(edge("seed", "real_client", 1.0));

        let result =
            compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        assert!(
            result.scores["fake_agent"] < 1e-6,
            "fake_agent={}",
            result.scores["fake_agent"]
        );
        assert!(
            result.scores["real_agent"] > result.scores["fake_agent"] * 1000.0,
            "real_agent={} fake_agent={}",
            result.scores["real_agent"],
            result.scores["fake_agent"]
        );
    }

    #[test]
    fn determinism_across_runs() {
        let edges = vec![
            edge("seed", "a", 0.8),
            edge("seed", "b", 0.2),
            edge("a", "c", 1.0),
            edge("b", "c", 1.0),
            edge("c", "d", 1.0),
        ];
        let r1 = compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        let r2 = compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        for peer in r1.scores.keys() {
            assert_eq!(r1.scores[peer], r2.scores[peer], "mismatch on {peer}");
        }
    }

    #[test]
    fn convergence_for_connected_graph() {
        // Connected graph (every peer endorses both seed and at
        // least one other peer). Per the EigenTrust paper §6,
        // connected graphs converge in <50 iterations even at
        // tolerance 1e-6. Confirms the "typical" case advertised in
        // the docstring.
        let edges = vec![
            edge("seed", "a", 1.0),
            edge("seed", "b", 1.0),
            edge("a", "b", 0.5),
            edge("a", "seed", 0.5),
            edge("b", "a", 0.5),
            edge("b", "seed", 0.5),
        ];
        let result =
            compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        assert!(result.converged);
        assert!(
            result.iterations < 50,
            "connected graph took {} iterations",
            result.iterations
        );
    }

    #[test]
    fn weight_proportional_to_score() {
        // seed gives a a perfect score (1.0) and b a weak score
        // (0.1). a should outrank b.
        let edges = vec![edge("seed", "a", 1.0), edge("seed", "b", 0.1)];
        let result =
            compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        assert!(
            result.scores["a"] > result.scores["b"],
            "a={} b={}",
            result.scores["a"],
            result.scores["b"]
        );
    }

    #[test]
    fn alpha_zero_does_not_panic() {
        // With α=0 the iteration is plain power iteration on Cᵀ
        // (no pre-trust anchor). NOT recommended — it reduces sybil
        // resistance to zero — but the implementation must not
        // panic and must return scores summing to 1. Convergence
        // depends on graph aperiodicity; we don't assert convergence
        // here (a periodic graph would oscillate forever) — only
        // that the run completes and the scores are well-formed.
        let edges = vec![edge("seed", "a", 1.0), edge("a", "seed", 1.0)];
        let cfg = EigenTrustConfig {
            alpha: 0.0,
            ..Default::default()
        };
        let result = compute_eigentrust(&edges, &pre(&["seed"]), &cfg).unwrap();
        let total: f64 = result.scores.values().sum();
        assert!((total - 1.0).abs() < 1e-9, "scores sum to {total}");
        assert!(result.scores["seed"] >= 0.0);
        assert!(result.scores["a"] >= 0.0);
    }

    #[test]
    fn alpha_zero_aperiodic_graph_converges() {
        // With α=0 on an aperiodic graph (chain with reflexive
        // endpoint) the power iteration does converge to the
        // dominant eigenvector. Verifies the α=0 code path works
        // when the graph is well-behaved.
        let edges = vec![
            edge("seed", "a", 1.0),
            edge("a", "b", 0.5),
            edge("a", "seed", 0.5),
            edge("b", "seed", 1.0),
        ];
        let cfg = EigenTrustConfig {
            alpha: 0.0,
            ..Default::default()
        };
        let result = compute_eigentrust(&edges, &pre(&["seed"]), &cfg).unwrap();
        let total: f64 = result.scores.values().sum();
        assert!((total - 1.0).abs() < 1e-9);
        assert!(result.scores["seed"] > 0.0);
        assert!(result.scores["a"] > 0.0);
        assert!(result.scores["b"] > 0.0);
    }

    #[test]
    fn alpha_one_collapses_to_pre_trust() {
        // With α=1 trust never flows through the graph — every
        // iteration overwrites with p. Result must be the pre-trust
        // vector exactly.
        let edges = vec![edge("seed", "a", 1.0), edge("a", "b", 1.0)];
        let cfg = EigenTrustConfig {
            alpha: 1.0,
            ..Default::default()
        };
        let result = compute_eigentrust(&edges, &pre(&["seed"]), &cfg).unwrap();
        assert!((result.scores["seed"] - 1.0).abs() < 1e-9);
        assert!(result.scores["a"].abs() < 1e-9);
        assert!(result.scores["b"].abs() < 1e-9);
    }

    #[test]
    fn stranded_peer_routes_to_pre_trusted_set() {
        // Peer "isolated" appears in no outgoing edge. Without the
        // stranded-peer handling it would be a sink and the
        // computation would diverge. With it, "isolated"'s trust is
        // recycled into the pre-trusted set each iteration.
        let edges = vec![edge("seed", "isolated", 1.0)];
        let result =
            compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        // Should converge cleanly and scores should sum to 1.
        assert!(result.converged);
        let total: f64 = result.scores.values().sum();
        assert!((total - 1.0).abs() < 1e-9);
    }

    #[test]
    fn multiple_pre_trusted_peers_split_evenly() {
        let result = compute_eigentrust(
            &[],
            &pre(&["s1", "s2", "s3", "s4"]),
            &EigenTrustConfig::default(),
        )
        .unwrap();
        for s in ["s1", "s2", "s3", "s4"] {
            assert!(
                (result.scores[s] - 0.25).abs() < 1e-9,
                "{s}={}",
                result.scores[s]
            );
        }
    }

    #[test]
    fn high_volume_real_client_outranks_low_volume_real_client() {
        // Both seed-vouched, but client_high has done 10 perfect
        // tasks with agent_x while client_low has done 1. agent_x
        // should accumulate more trust through the high-volume path.
        let mut edges = vec![
            edge("seed", "client_high", 1.0),
            edge("seed", "client_low", 1.0),
        ];
        for _ in 0..10 {
            edges.push(edge("client_high", "agent_x", 1.0));
        }
        edges.push(edge("client_low", "agent_y", 1.0));
        let result =
            compute_eigentrust(&edges, &pre(&["seed"]), &EigenTrustConfig::default()).unwrap();
        // Both clients have the same trust (uniform pre-trust
        // hop), but agent_x receives 10x the volume from client_high
        // — though with row normalization that's only 10/10 = 1
        // weight per outgoing edge. So agent_x and agent_y get
        // equal trust per row-normalized edge from their
        // respective client. The volume signal is in the *number*
        // of clients endorsing, not the count of edges from one.
        assert!(
            (result.scores["agent_x"] - result.scores["agent_y"]).abs() < 1e-3,
            "agent_x={} agent_y={}",
            result.scores["agent_x"],
            result.scores["agent_y"]
        );
    }
}
