#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

//! The chain registry — single source of truth for every per-chain value
//! (same rule as `game-constants`). Adding a chain is an entry here, not
//! a code change anywhere else. Per the root CLAUDE.md "Multichain
//! Frameworks" standard: no per-chain hardcoded constants outside this
//! crate.

use chain_core::{ChainId, Namespace};

/// How "final" is defined on a chain — the deploy-precondition finality
/// table from `multichain/decision.md` §4.1. Client quorum reads pin to
/// this level before signing a match-live certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finality {
    /// Solana `finalized` commitment.
    SolanaFinalized,
    /// EVM `finalized` block tag (post-merge two-epoch finality; on an
    /// L2 this is the sequencer's finalized tag — L1-posting nuance is
    /// testnet-acceptable and revisited at the mainnet gate).
    EvmFinalizedTag,
}

/// One chain's complete configuration.
#[derive(Debug, Clone)]
pub struct ChainEntry {
    /// CAIP-2 string, e.g. `eip155:84532`.
    pub chain_id: &'static str,
    pub display_name: &'static str,
    /// Independent RPC endpoints for M-of-N quorum reads. Disagreement
    /// at the pinned finality level → refuse to sign match-live.
    pub rpc_urls: &'static [&'static str],
    /// Minimum agreeing providers for a quorum read.
    pub quorum_m: usize,
    pub finality: Finality,
    pub native_symbol: &'static str,
    pub native_decimals: u8,
    /// Per-match stake in native base units (lamports / wei), tuned to
    /// rough USD parity across chains (config, not oracle).
    pub stake_base_units: u128,
    /// Float-pool per-match tranche clamp (panel requirement A3).
    pub max_tranche_base_units: u128,
    /// Contested-claim window. Claims close at match_deadline + this;
    /// refundTimeout opens 2×skew_margin_secs later on BOTH legs.
    pub claim_window_secs: u32,
    pub skew_margin_secs: u32,
    /// Coordination-game program ID (solana) or CrossChainGame contract
    /// address (eip155). None until deployed on that chain.
    pub game_contract: Option<&'static str>,
    /// x402 network descriptor name, when this chain settles payments.
    pub x402_network: Option<&'static str>,
}

/// Testnet stake parity note: 0.05 SOL and 0.0025 ETH are within the
/// same rough-USD band; exact parity is intentionally NOT enforced
/// (rates are agreed per-match in the co-signed schedule).
const REGISTRY: &[ChainEntry] = &[
    ChainEntry {
        chain_id: "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1",
        display_name: "Solana Devnet",
        rpc_urls: &["https://api.devnet.solana.com"],
        quorum_m: 1,
        finality: Finality::SolanaFinalized,
        native_symbol: "SOL",
        native_decimals: 9,
        stake_base_units: 50_000_000, // 0.05 SOL — FIXED_STAKE_LAMPORTS
        max_tranche_base_units: 100_000_000, // 0.1 SOL
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        game_contract: Some("2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"),
        x402_network: None,
    },
    ChainEntry {
        chain_id: "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp",
        display_name: "Solana Mainnet",
        rpc_urls: &["https://api.mainnet-beta.solana.com"],
        quorum_m: 1,
        finality: Finality::SolanaFinalized,
        native_symbol: "SOL",
        native_decimals: 9,
        stake_base_units: 50_000_000,
        max_tranche_base_units: 100_000_000,
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        game_contract: Some("2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"),
        x402_network: Some("solana"),
    },
    ChainEntry {
        chain_id: "eip155:84532",
        display_name: "Base Sepolia",
        rpc_urls: &[
            "https://sepolia.base.org",
            "https://base-sepolia-rpc.publicnode.com",
            "https://base-sepolia.drpc.org",
        ],
        quorum_m: 2,
        finality: Finality::EvmFinalizedTag,
        native_symbol: "ETH",
        native_decimals: 18,
        // Testnet: MUST match the deployed CrossChainGame's stakeWei/maxTrancheWei
        // (0.00001 / 0.00002 ETH) — the contract was deployed with tiny stakes to
        // conserve testnet ETH, and createMatch records exactly stakeWei, so the
        // cert's leg_b.stake must equal it or settle's digest check fails. This
        // breaks the 0.05-SOL↔ETH parity intended for mainnet; real parity needs
        // the operator rate-quote mechanism (see decision.md / 5d-H) + a mainnet
        // contract deployed with the parity stake. Verified on-chain 2026-06-13.
        stake_base_units: 10_000_000_000_000, // 0.00001 ETH (== deployed stakeWei)
        max_tranche_base_units: 20_000_000_000_000, // 0.00002 ETH (== deployed maxTrancheWei)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        // CrossChainGame redeployed 2026-06-14 with permissionless lockTranche
        // (operatorSigner 0x54a6…9A30 verified on-chain; prior 0xC2eb…62AFf orphaned).
        game_contract: Some("0xd585baE48901513202dAEb7d4feE4Af508a96234"),
        x402_network: Some("base-sepolia"),
    },
];

/// Look up a chain's configuration. None = chain not supported; callers
/// at system boundaries reject rather than guess.
pub fn entry(chain: &ChainId) -> Option<&'static ChainEntry> {
    REGISTRY.iter().find(|e| e.chain_id == chain.as_str())
}

/// Every registered chain, in registry order. Callers that need cross-chain
/// discovery (e.g. the MCP `xchain_supported_chains` tool) iterate this rather
/// than hardcoding the chain set.
pub fn all() -> impl Iterator<Item = &'static ChainEntry> {
    REGISTRY.iter()
}

/// The Solana leg of the testnet cross-chain game (devnet) — the partner chain
/// for the EVM testnet leg (Base Sepolia). Cross-chain is testnet-only until
/// the mainnet gate (decision.md §6), so callers resolving a base58 Solana
/// wallet to a CAIP-2 chain use this single source of truth rather than
/// hardcoding the devnet id. Returns `None` only if the devnet entry is ever
/// removed.
pub fn cross_chain_solana() -> Option<&'static ChainEntry> {
    ChainId::parse("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1")
        .ok()
        .and_then(|id| entry(&id))
}

/// All registered chains in a namespace.
pub fn entries_for(namespace: Namespace) -> impl Iterator<Item = &'static ChainEntry> {
    REGISTRY.iter().filter(move |e| {
        ChainId::parse(e.chain_id)
            .map(|c| c.namespace() == namespace)
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registry_entry_has_a_valid_caip2_id() {
        for e in REGISTRY {
            let chain = ChainId::parse(e.chain_id)
                .unwrap_or_else(|err| panic!("{}: invalid CAIP-2: {err}", e.chain_id));
            assert_eq!(chain.as_str(), e.chain_id);
        }
    }

    #[test]
    fn quorum_never_exceeds_provider_count_and_is_nonzero() {
        for e in REGISTRY {
            assert!(e.quorum_m >= 1, "{}: quorum must be >= 1", e.chain_id);
            assert!(
                e.quorum_m <= e.rpc_urls.len(),
                "{}: quorum {} exceeds {} providers",
                e.chain_id,
                e.quorum_m,
                e.rpc_urls.len()
            );
        }
    }

    #[test]
    fn cross_chain_solana_is_a_registered_devnet_entry() {
        let e = cross_chain_solana().expect("devnet solana registered");
        assert_eq!(e.native_symbol, "SOL");
        assert_eq!(e.display_name, "Solana Devnet");
        // Must be a real registry entry (not mainnet).
        assert!(e.x402_network.is_none(), "cross-chain solana leg is devnet");
    }

    #[test]
    fn all_returns_every_entry() {
        let count = all().count();
        assert_eq!(count, REGISTRY.len());
        // all() must be the union of the per-namespace views.
        let by_ns = entries_for(Namespace::Solana).count() + entries_for(Namespace::Eip155).count();
        assert_eq!(
            count, by_ns,
            "a chain exists in a namespace all() doesn't sum"
        );
    }

    #[test]
    fn lookup_by_chain_id_and_namespace() {
        let base = ChainId::parse("eip155:84532").unwrap();
        let entry = entry(&base).expect("base sepolia registered");
        assert_eq!(entry.native_symbol, "ETH");
        assert_eq!(entry.quorum_m, 2);

        let solana_count = entries_for(Namespace::Solana).count();
        assert_eq!(solana_count, 2);
        let evm_count = entries_for(Namespace::Eip155).count();
        assert_eq!(evm_count, 1);

        let unknown = ChainId::parse("eip155:1").unwrap();
        assert!(super::entry(&unknown).is_none());
    }

    #[test]
    fn stakes_and_tranches_are_positive_with_sane_windows() {
        for e in REGISTRY {
            assert!(e.stake_base_units > 0, "{}: zero stake", e.chain_id);
            assert!(
                e.max_tranche_base_units >= e.stake_base_units,
                "{}: tranche clamp below stake makes winner-takes unpayable",
                e.chain_id
            );
            // Panel requirement: claim windows bounded (≤1h target) and
            // skew margin nonzero.
            assert!(e.claim_window_secs <= 3_600, "{}: window > 1h", e.chain_id);
            assert!(
                e.skew_margin_secs >= 60,
                "{}: skew margin too small",
                e.chain_id
            );
        }
    }
}
