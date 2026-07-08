//! Emit the LIVE EVM CrossChainGame entries so a CI job can verify the
//! registry's stake/tranche match what's actually deployed on-chain (finding
//! #6 — close the comment-only "keep these in lockstep" gap with a real guard).
//!
//! One tab-separated line per live EVM entry:
//!   `<chain_id>\t<contract>\t<stake_base_units>\t<max_tranche_base_units>\t<rpc0,rpc1,...>`
//!
//! Only entries whose CrossChainGame contract is deployed (`is_live`) are
//! emitted — scaffolded mainnet chains (contract = None) have nothing on-chain
//! to compare against yet, so they're skipped until their deploy lands.

use chain_core::{ChainId, Namespace};
use chain_registry::{all, ContractPurpose};

fn main() {
    for e in all() {
        let is_evm = ChainId::parse(e.chain_id)
            .map(|c| c.namespace() == Namespace::Eip155)
            .unwrap_or(false);
        if !is_evm || !e.is_live(ContractPurpose::CrossChainGame) {
            continue;
        }
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
