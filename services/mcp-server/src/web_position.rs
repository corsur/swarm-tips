//! Web-position scoring (B2) — the extension-credit reputation graph.
//!
//! Turns the active extension edges (extender → recipient vouches) into
//! per-agent web-position scores via the EigenTrust crate, anchored to the
//! single trusted root (the deploy/treasury wallet). Scores are normalized to
//! 0..1 for use as the `credit_web` signal in `composite_trust`.
//!
//! On-demand by design for the MVP: the dogfood graph is tiny, so the MCP
//! server computes positions synchronously when asked (like its other on-chain
//! reads — no background loop, no persistent state, KEDA-safe). A
//! Firestore-cached, Workflow-recomputed indexer is the scale-time evolution
//! (deferred, mirroring EigenTrust task #35 for the output-reputation graph).

use std::collections::{HashMap, HashSet};

use eigentrust::{compute_eigentrust, EigenTrustConfig, TrustEdge};

/// One active extension: `extender` vouches for `recipient`. The bond is the
/// edge-weight signal (a larger bond is a stronger vouch); a 1.0 floor keeps
/// every active extension contributing regardless of bond size.
#[derive(Debug, Clone)]
pub struct ExtensionEdge {
    pub extender: String,
    pub recipient: String,
    pub bond_lamports: u64,
}

/// Compute per-agent web-position scores (0..1) from the active extension
/// edges, anchored to `root`. The root itself is excluded from the result.
///
/// Raw EigenTrust scores sum to 1.0 across all peers, so any single agent's
/// raw score is small; we rescale by the maximum non-root score so the
/// best-standing agent maps to ~1.0 — the 0..1 domain the `credit_web` signal
/// expects. Returns an empty map when there are no edges or no positive
/// non-root score.
pub fn compute_web_positions(extensions: &[ExtensionEdge], root: &str) -> HashMap<String, f64> {
    if extensions.is_empty() {
        return HashMap::new();
    }
    let edges: Vec<TrustEdge> = extensions
        .iter()
        .filter(|e| e.extender != e.recipient)
        .map(|e| TrustEdge {
            from: e.extender.clone(),
            to: e.recipient.clone(),
            weight: 1.0 + (e.bond_lamports as f64) / 1_000_000_000.0,
        })
        .collect();
    if edges.is_empty() {
        return HashMap::new();
    }

    let mut pre_trusted: HashSet<String> = HashSet::new();
    pre_trusted.insert(root.to_string());

    let config = EigenTrustConfig::default();
    let result = match compute_eigentrust(&edges, &pre_trusted, &config) {
        Ok(r) => r,
        Err(e) => {
            // An empty map is also the legitimate "no edges" answer, so a
            // silent return here made a computation failure indistinguishable
            // from a genuinely empty credit web.
            tracing::warn!(root = %root, error = ?e, "EigenTrust computation failed — returning empty web positions");
            return HashMap::new();
        }
    };

    // Keep `root` present during SCC condensation capping so direct-anchor
    // external inflow is computed from `scores[root]` with exact `(1 - alpha)`
    // row-normalized decay, preventing root-vouched `A <-> Sybil` mutual-vouch
    // rings from inflating above `A`'s direct root allocation (P2-28).
    let mut scores: HashMap<String, f64> = result.scores;
    reputation_indexer::cap_at_entry_points(&mut scores, &edges, &pre_trusted, config.alpha);
    scores.remove(root);

    let max_non_root = scores
        .values()
        .copied()
        .filter(|s| s.is_finite())
        .fold(0.0_f64, f64::max);
    if !max_non_root.is_finite() || max_non_root <= 0.0 {
        return HashMap::new();
    }
    scores
        .into_iter()
        .map(|(id, s)| {
            let norm = if s.is_finite() {
                (s / max_non_root).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (id, norm)
        })
        .collect()
}

/// Root of trust for web-position (B6 — the deploy / treasury wallet). All
/// positions are computed relative to it.
pub const WEB_POSITION_ROOT: &str = "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";

/// Read the live extension graph from `rpc_url` and return
/// `(web_position, received_extension_count)` for `agent`. Shared by the
/// `agent_trust_score` MCP tool and the `/internal/agent-reputation` HTTP
/// endpoint. Returns `(None, 0)` on RPC error, no extensions, or an agent that
/// has received none.
pub async fn agent_web_position(
    client: &reqwest::Client,
    rpc_url: &str,
    agent: &str,
) -> (Option<f64>, u64) {
    match try_agent_web_position(client, rpc_url, agent).await {
        Ok(position) => position,
        Err(error) => {
            tracing::warn!(%error,"optional credit-web signal unavailable");
            (None, 0)
        }
    }
}

pub async fn try_agent_web_position(
    client: &reqwest::Client,
    rpc_url: &str,
    agent: &str,
) -> anyhow::Result<(Option<f64>, u64)> {
    let extensions = crate::solana_reads::read_all_extensions(client, rpc_url).await?;
    if extensions.is_empty() {
        return Ok((None, 0));
    }
    let received = extensions.iter().filter(|e| e.recipient == agent).count() as u64;
    if received == 0 {
        return Ok((None, 0));
    }
    let edges: Vec<ExtensionEdge> = extensions
        .into_iter()
        .map(|e| ExtensionEdge {
            extender: e.extender,
            recipient: e.recipient,
            bond_lamports: e.bond_lamports,
        })
        .collect();
    let position = compute_web_positions(&edges, WEB_POSITION_ROOT)
        .get(agent)
        .copied();
    Ok((position, received))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "root";

    fn ext(extender: &str, recipient: &str) -> ExtensionEdge {
        ExtensionEdge {
            extender: extender.into(),
            recipient: recipient.into(),
            bond_lamports: 5_000_000,
        }
    }

    #[test]
    fn empty_extensions_yield_no_positions() {
        assert!(compute_web_positions(&[], ROOT).is_empty());
    }

    #[test]
    fn root_vouched_agent_gets_top_position() {
        // root → a → b: a (directly vouched by the root) outranks b (one more hop).
        let exts = vec![ext(ROOT, "a"), ext("a", "b")];
        let pos = compute_web_positions(&exts, ROOT);
        assert!(pos["a"] > 0.0 && pos["b"] > 0.0);
        assert!(pos["a"] >= pos["b"]);
        // Best non-root agent normalizes to 1.0; root is excluded.
        let max = pos.values().cloned().fold(0.0, f64::max);
        assert!((max - 1.0).abs() < 1e-9);
        assert!(!pos.contains_key(ROOT));
    }

    #[test]
    fn sybil_cluster_unreachable_from_root_gets_near_zero() {
        // root → real; sybils only vouch for each other → unreachable from root.
        let exts = vec![ext(ROOT, "real"), ext("s_a", "s_b"), ext("s_b", "s_a")];
        let pos = compute_web_positions(&exts, ROOT);
        assert!((pos["real"] - 1.0).abs() < 1e-9, "real normalizes to ~1");
        for s in ["s_a", "s_b"] {
            assert!(pos.get(s).copied().unwrap_or(0.0) < 0.01, "{s} near zero");
        }
    }

    #[test]
    fn root_vouched_sybil_mutual_vouch_ring_cannot_inflate_or_crush_honest_agent() {
        // P2-28 regression: root vouches equally for `honest` and `a`, and `a`
        // spins up a high-bond mutual-vouch ring `a <-> sybil` plus a self-loop
        // `a -> a`. Under the legacy widest-path `f64::INFINITY` cap, `a` and
        // `sybil` trapped random-walk mass and crushed `honest` below 0.5.
        // With Tarjan SCC condensation capping, `{a, sybil}` is capped strictly
        // to `a`'s direct root allocation, so `honest` retains top standing (~1.0)
        // and neither `a` nor `sybil` can outrank `honest`.
        let exts = vec![
            ext(ROOT, "honest"),
            ext(ROOT, "a"),
            ExtensionEdge {
                extender: "a".into(),
                recipient: "sybil".into(),
                bond_lamports: 100_000_000_000,
            },
            ExtensionEdge {
                extender: "sybil".into(),
                recipient: "a".into(),
                bond_lamports: 100_000_000_000,
            },
            ExtensionEdge {
                extender: "a".into(),
                recipient: "a".into(),
                bond_lamports: 100_000_000_000,
            },
        ];
        let pos = compute_web_positions(&exts, ROOT);
        assert!(
            (pos["honest"] - 1.0).abs() < 1e-9,
            "honest agent must not be crushed by a root-vouched Sybil ring, got {}",
            pos["honest"]
        );
        assert!(
            pos["a"] <= pos["honest"] + 1e-9,
            "ring entry `a` ({}) must not outrank equally vouched `honest` ({})",
            pos["a"],
            pos["honest"]
        );
        assert!(
            pos["sybil"] <= pos["honest"] + 1e-9,
            "sybil ({}) must not outrank `honest` ({})",
            pos["sybil"],
            pos["honest"]
        );
    }
}
