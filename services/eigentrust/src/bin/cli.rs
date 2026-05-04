#![deny(warnings)]
#![deny(clippy::all)]

//! Offline EigenTrust runner. Reads a JSON document from stdin,
//! prints the per-peer global trust scores as JSON to stdout.
//!
//! Input shape:
//! ```json
//! {
//!   "edges": [{"from": "client_a", "to": "agent_b", "weight": 0.85}, ...],
//!   "pre_trusted": ["seed_1", "seed_2"],
//!   "alpha": 0.1,
//!   "max_iterations": 100,
//!   "tolerance": 1e-6
//! }
//! ```
//!
//! `alpha`, `max_iterations`, `tolerance` are optional and fall back
//! to `EigenTrustConfig::default()`.
//!
//! Output shape:
//! ```json
//! {
//!   "scores": {"client_a": 0.42, "agent_b": 0.31, ...},
//!   "iterations": 23,
//!   "converged": true
//! }
//! ```

use std::collections::HashSet;
use std::io::Read;

use eigentrust::{compute_eigentrust, EigenTrustConfig, PeerId, TrustEdge};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Input {
    edges: Vec<TrustEdge>,
    pre_trusted: Vec<PeerId>,
    #[serde(default)]
    alpha: Option<f64>,
    #[serde(default)]
    max_iterations: Option<usize>,
    #[serde(default)]
    tolerance: Option<f64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    let input: Input = serde_json::from_str(&buf)?;

    let default = EigenTrustConfig::default();
    let config = EigenTrustConfig {
        alpha: input.alpha.unwrap_or(default.alpha),
        max_iterations: input.max_iterations.unwrap_or(default.max_iterations),
        tolerance: input.tolerance.unwrap_or(default.tolerance),
    };

    let pre_trusted: HashSet<PeerId> = input.pre_trusted.into_iter().collect();
    let result = compute_eigentrust(&input.edges, &pre_trusted, &config)?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
