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
        .map(|e| TrustEdge {
            from: e.extender.clone(),
            to: e.recipient.clone(),
            weight: 1.0 + (e.bond_lamports as f64) / 1_000_000_000.0,
        })
        .collect();

    let mut pre_trusted: HashSet<String> = HashSet::new();
    pre_trusted.insert(root.to_string());

    let result = match compute_eigentrust(&edges, &pre_trusted, &EigenTrustConfig::default()) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };

    let max_non_root = result
        .scores
        .iter()
        .filter(|(id, _)| id.as_str() != root)
        .map(|(_, s)| *s)
        .fold(0.0_f64, f64::max);
    if max_non_root <= 0.0 {
        return HashMap::new();
    }

    result
        .scores
        .into_iter()
        .filter(|(id, _)| id != root)
        .map(|(id, s)| (id, (s / max_non_root).clamp(0.0, 1.0)))
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
    let extensions = match crate::solana_reads::read_all_extensions(client, rpc_url).await {
        Ok(e) => e,
        Err(_) => return (None, 0),
    };
    if extensions.is_empty() {
        return (None, 0);
    }
    let received = extensions.iter().filter(|e| e.recipient == agent).count() as u64;
    if received == 0 {
        return (None, 0);
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
    (position, received)
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
}
