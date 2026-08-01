//! Emit the LIVE EVM CrossChainGame entries so a CI job can verify the
//! registry's stake/tranche match what's actually deployed on-chain (finding
//! #6 — close the comment-only "keep these in lockstep" gap with a real guard).
//!
//! One tab-separated line per live EVM entry:
//!   `<chain_id>\t<contract>\t<stake_base_units>\t<max_tranche_base_units>\t<rpc0,rpc1,...>`
//!
//! Only entries whose CrossChainGame contract is deployed are emitted. That
//! filter is currently a no-op for EVM: every registered eip155 entry now has a
//! contract, including both mainnets — the "scaffolded mainnet chains are
//! skipped" note this carried predates those deploys. It stays because a newly
//! scaffolded chain must not break the parity job on day one.

use chain_core::{ChainId, Namespace};
use chain_registry::{all, ContractPurpose};

fn main() {
    for e in all() {
        let is_evm = ChainId::parse(e.chain_id)
            .map(|c| c.namespace() == Namespace::Eip155)
            .unwrap_or(false);
        if !is_evm {
            continue;
        }
        // This let-else IS the liveness check: `is_live(p)` is defined as
        // `contract_for(p).is_some()`, so guarding on both made the second
        // branch unreachable.
        let Some(contract) = e.contract_for(ContractPurpose::CrossChainGame) else {
            continue;
        };
        println!(
            "{}\t{}\t{}\t{}\t{}",
            e.chain_id,
            contract,
            e.stake_base_units,
            e.max_tranche_base_units,
            e.rpc_urls.join(","),
        );
    }
}
