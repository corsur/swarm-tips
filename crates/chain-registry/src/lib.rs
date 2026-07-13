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

/// A deployed contract's role on a chain, so one chain can expose several (an
/// EVM chain hosts BOTH the cross-chain `CrossChainGame` and the same-chain
/// `CoordinationGame`). The generalized lookup every product shares: adding a
/// future product (e.g. a Shillbot EVM escrow) is a new variant + an address on
/// the relevant entries — no consumer change. See `contract_for`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractPurpose {
    /// The cross-chain match contract — Solana coordination-game program, or the
    /// EVM `CrossChainGame`.
    CrossChainGame,
    /// The same-chain EVM-vs-EVM `CoordinationGame`.
    CoordinationGame,
    /// The Shillbot task-escrow — Solana shillbot program, or the EVM
    /// `ShillbotEscrow`.
    ShillbotEscrow,
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
    /// True for real-money chains (Solana mainnet, Base, Ethereum). Gates
    /// real-money-only guards — e.g. the cross-chain FX sanity band, which is
    /// meaningless on testnets whose stakes are nominal rather than USD-tuned.
    pub is_mainnet: bool,
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
    /// Cross-chain match contract: coordination-game program ID (solana) or
    /// `CrossChainGame` address (eip155). None until deployed on that chain.
    /// Prefer `contract_for(ContractPurpose::CrossChainGame)` in new code.
    pub game_contract: Option<&'static str>,
    /// Same-chain EVM-vs-EVM `CoordinationGame` address (eip155). None where it
    /// isn't deployed — every Solana entry, and an EVM chain until deployed.
    pub coordination_game_contract: Option<&'static str>,
    /// Shillbot task-escrow: shillbot program ID (solana) or `ShillbotEscrow`
    /// address (eip155). None until deployed on that chain (the EVM address
    /// lands in a follow-up commit after the gated testnet deploy).
    pub shillbot_escrow_contract: Option<&'static str>,
    /// x402 network descriptor name, when this chain settles payments.
    pub x402_network: Option<&'static str>,
}

impl ChainEntry {
    /// The deployed contract for a purpose on this chain, if any. The
    /// generalized lookup all products share — a future product is a new
    /// `ContractPurpose` variant mapped to its address field.
    pub fn contract_for(&self, purpose: ContractPurpose) -> Option<&'static str> {
        match purpose {
            ContractPurpose::CrossChainGame => self.game_contract,
            ContractPurpose::CoordinationGame => self.coordination_game_contract,
            ContractPurpose::ShillbotEscrow => self.shillbot_escrow_contract,
        }
    }

    /// Whether a product is LIVE (deployed) on this chain. A deployed contract
    /// address IS the deployment truth — a chain scaffolded here with a `None`
    /// address is "coming soon", not playable. Backend and frontend gate every
    /// real transaction on this; no separate `deployed` bool that could drift
    /// from the address (single source of truth). A mainnet entry ships with
    /// `None` and flips live the moment the follow-up commit records its address.
    pub fn is_live(&self, purpose: ContractPurpose) -> bool {
        self.contract_for(purpose).is_some()
    }
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
        is_mainnet: false,
        native_symbol: "SOL",
        native_decimals: 9,
        // Cross-chain leg parity with the $5 EVM anchor (0.0032 ETH): ~0.068 SOL
        // at SOL≈$73. Only the xchain Solana leg reads this (same-chain Solana
        // uses the on-chain FIXED_STAKE constant); create_xmatch takes stake as
        // an arg (no fixed on-chain stake), and 0.068 ≤ the live xpool's 0.1-SOL
        // max_tranche, so no Solana redeploy or pool reconfig is needed.
        stake_base_units: 68_000_000, // ~0.068 SOL ($5 parity with the EVM leg)
        max_tranche_base_units: 100_000_000, // 0.1 SOL (unchanged; == live xpool max_tranche)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        game_contract: Some("2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"),
        coordination_game_contract: None, // same-chain EVM game has no Solana deployment
        shillbot_escrow_contract: None,   // not yet deployed on this chain
        x402_network: None,
    },
    ChainEntry {
        chain_id: "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp",
        display_name: "Solana Mainnet",
        rpc_urls: &["https://api.mainnet-beta.solana.com"],
        quorum_m: 1,
        finality: Finality::SolanaFinalized,
        is_mainnet: true,
        native_symbol: "SOL",
        native_decimals: 9,
        stake_base_units: 50_000_000,
        max_tranche_base_units: 100_000_000,
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        game_contract: Some("2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"),
        coordination_game_contract: None,
        shillbot_escrow_contract: None, // not yet deployed on this chain
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
        is_mainnet: false,
        native_symbol: "ETH",
        native_decimals: 18,
        // Testnet: MUST match the deployed CrossChainGame's stakeWei/maxTrancheWei
        // — createMatch records exactly stakeWei, so the cert's leg_b.stake must
        // equal it or settle's digest check fails. Set to the SETTLED $5 anchor
        // (0.0032 / 0.0064 ETH) at the audit-fix redeploy; the Solana devnet leg
        // below is sized to match (~0.068 SOL ≈ $5). Bump both the deploy config
        // (deploy-evm-testnet.yml XCHAIN_STAKE_WEI/XCHAIN_MAX_TRANCHE_WEI) and
        // this entry in lockstep — a registry-only change breaks the live e2e.
        stake_base_units: 3_200_000_000_000_000, // 0.0032 ETH ($5 anchor, == deployed stakeWei)
        max_tranche_base_units: 6_400_000_000_000_000, // 0.0064 ETH (== deployed maxTrancheWei)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        // CrossChainGame redeployed 2026-07-01 with the audit-fix hardening
        // (M1 pull-payment, M2/M4 snapshots + createMatch operator-sig, L1 config
        // bounds) at the $5 stake (operatorSigner 0x54a6…9A30; prior tiny-stake
        // 0xd585…6234 orphaned by this redeploy).
        game_contract: Some("0xd38b1fB07Bf64801bCBc3721937D6e2Ba6E5feb4"),
        // Same-chain EVM-vs-EVM CoordinationGame, redeployed 2026-07-01 with the
        // audit-fix hardening (M1/M3/H3/L1/L2) at the $5 stake (operatorSigner
        // 0x54a6…9A30 == game-api xchain-operator-signer, so game-api's v=27/28
        // normalized createGame attestation verifies; prior 0x2F88…4664 orphaned).
        coordination_game_contract: Some("0x042fE7202d208C9D79AdFd276da77b928C64514b"),
        // ShillbotEscrow deployed 2026-07-07 (S5 live demo): chainTag
        // keccak256("eip155:84532"), attesterSigner is a demo key — rotate to
        // the dedicated shillbot-attester EVM key via setConfig before real use.
        shillbot_escrow_contract: Some("0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c"),
        x402_network: Some("base-sepolia"),
    },
    // Ethereum Sepolia — second EVM testnet, added for full testnet parity so
    // every game combination (same-chain EVM + Solana↔Ethereum / Base↔Ethereum
    // cross-chain) is verifiable with test funds, no real money. SCAFFOLDED:
    // contracts are None until the `ethereum_sepolia` deploy lands (dispatch
    // deploy-evm-testnet.yml network=ethereum_sepolia), at which point a
    // follow-up commit records the addresses and the parity guard validates
    // stakeWei/maxTrancheWei. Stake matches Base Sepolia's $5 testnet anchor
    // (== _deploy-evm.yml ethereum_sepolia XCHAIN_STAKE_WEI, in lockstep).
    ChainEntry {
        chain_id: "eip155:11155111",
        display_name: "Ethereum Sepolia",
        rpc_urls: &[
            "https://ethereum-sepolia-rpc.publicnode.com",
            "https://eth-sepolia.public.blastapi.io",
            "https://sepolia.drpc.org",
        ],
        quorum_m: 2,
        finality: Finality::EvmFinalizedTag,
        is_mainnet: false,
        native_symbol: "ETH",
        native_decimals: 18,
        stake_base_units: 3_200_000_000_000_000, // 0.0032 ETH ($5 anchor, == deploy stakeWei)
        max_tranche_base_units: 6_400_000_000_000_000, // 0.0064 ETH (2× stake)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        // Filled by the post-deploy follow-up commit (is_live stays false until then).
        game_contract: None,
        coordination_game_contract: None,
        shillbot_escrow_contract: None,
        x402_network: None,
    },
    // ── Mainnet EVM chains (scaffolded, NOT yet live) ────────────────────────
    // All contract addresses are None until the gated mainnet deploy lands; a
    // follow-up commit records each deployed address (verified against the
    // on-chain stakeWei/maxTrancheWei by the evm-ci parity guard). Until then
    // `is_live(..)` is false and no code path may build a real tx for them.
    // Stakes below are the $5-anchor CONFIG (re-tuned at the gated deploy to the
    // then-current ETH price); tranche is 2× stake. They must match the deploy
    // workflow's XCHAIN_STAKE_WEI/XCHAIN_MAX_TRANCHE_WEI for that network in
    // lockstep — the parity guard fails CI on divergence once deployed.
    ChainEntry {
        chain_id: "eip155:8453",
        display_name: "Base",
        rpc_urls: &[
            "https://mainnet.base.org",
            "https://base-rpc.publicnode.com",
            "https://base.drpc.org",
        ],
        quorum_m: 2,
        finality: Finality::EvmFinalizedTag,
        is_mainnet: true,
        native_symbol: "ETH",
        native_decimals: 18,
        // Low launch stake (~$1.50 at ETH≈$3000) for the initial minimal-budget
        // Base-mainnet run (~20 games). Base L2 gas is cents, so even a tiny stake
        // is economically sane here (gas ≪ stake). Bump post-launch as desired;
        // keep in lockstep with the deploy workflow's base XCHAIN_STAKE_WEI.
        stake_base_units: 500_000_000_000_000, // 0.0005 ETH
        max_tranche_base_units: 1_000_000_000_000_000, // 0.001 ETH (2× stake)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        // Cross-chain CrossChainGame deployed to Base mainnet 2026-07-11 (0.0005
        // ETH stake; owner/operatorSigner = xchain key / xchain-operator-signer).
        game_contract: Some("0xC2DbD950400965b3f4d9A4D6B1af4a0eb65CC365"),
        // Same-chain CoordinationGame deployed to Base mainnet 2026-07-09 (0.0005
        // ETH launch stake; owner/operatorSigner = xchain key / xchain-operator-signer).
        coordination_game_contract: Some("0x778F6FDA3ffd0ecD3708FDa0E6c2940606529fe9"),
        shillbot_escrow_contract: None,
        x402_network: Some("base"),
    },
    ChainEntry {
        chain_id: "eip155:1",
        display_name: "Ethereum",
        rpc_urls: &[
            "https://ethereum-rpc.publicnode.com",
            "https://eth.drpc.org",
            "https://cloudflare-eth.com",
        ],
        quorum_m: 2,
        finality: Finality::EvmFinalizedTag,
        is_mainnet: true,
        native_symbol: "ETH",
        native_decimals: 18,
        // Anchored to the decided cross-chain peg: 0.0025 ETH ≈ $4 ≈ 0.05 SOL
        // (decision.md §4.1), so an Ethereum same-chain game costs the same as
        // Solana. For CROSS-chain matches the Solana leg is priced dynamically
        // off this EVM anchor at match time (game-api `build_xmatch_terms`), so
        // both legs are always equal in live USD. Re-tune this literal only at
        // (re)deploy; keep it in lockstep with `_deploy-evm.yml` ethereum stake.
        // Note: L1 gas can rival the stake during congestion — acceptable per
        // founder (uniform pricing preferred over gas-minimized L1 stakes).
        stake_base_units: 2_500_000_000_000_000, // 0.0025 ETH
        max_tranche_base_units: 5_000_000_000_000_000, // 0.005 ETH (2× stake)
        claim_window_secs: 3_600,
        skew_margin_secs: 900,
        // Both deployed to Ethereum L1 mainnet 2026-07-11 (0.0025 ETH stake;
        // owner/operatorSigner = xchain key / xchain-operator-signer).
        game_contract: Some("0x5E9eb986927bDF70F2f9fE5BccAFF3dEE74949EB"),
        coordination_game_contract: Some("0xd52adFf967F7efbf2Bbb29252E0aA219bD5305B3"),
        shillbot_escrow_contract: None,
        x402_network: None,
    },
];

/// Look up a chain's configuration. None = chain not supported; callers
/// at system boundaries reject rather than guess.
pub fn entry(chain: &ChainId) -> Option<&'static ChainEntry> {
    REGISTRY.iter().find(|e| e.chain_id == chain.as_str())
}

/// The deployed contract for a (chain, purpose) — None if the chain is
/// unregistered or that contract isn't deployed there. The single lookup every
/// product (game today, a future Shillbot EVM escrow tomorrow) shares.
pub fn contract_for(chain: &ChainId, purpose: ContractPurpose) -> Option<&'static str> {
    entry(chain).and_then(|e| e.contract_for(purpose))
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
        // Base Sepolia + Ethereum Sepolia + Base mainnet + Ethereum mainnet.
        let evm_count = entries_for(Namespace::Eip155).count();
        assert_eq!(evm_count, 4);

        // eip155:1 is now Ethereum mainnet (registered); use an unregistered id.
        let unknown = ChainId::parse("eip155:999999").unwrap();
        assert!(super::entry(&unknown).is_none());
    }

    #[test]
    fn is_mainnet_matches_known_networks() {
        let m = |c: &str| entry(&ChainId::parse(c).unwrap()).unwrap().is_mainnet;
        assert!(!m("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"), "devnet");
        assert!(!m("eip155:84532"), "base sepolia");
        assert!(
            m("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"),
            "solana mainnet"
        );
        assert!(m("eip155:8453"), "base mainnet");
        assert!(m("eip155:1"), "ethereum mainnet");
    }

    #[test]
    fn mainnet_evm_chains_go_live_only_where_deployed() {
        // Ethereum L1: both game contracts deployed 2026-07-11 → live; shillbot
        // escrow not deployed there, stays None.
        let eth = entry(&ChainId::parse("eip155:1").unwrap()).unwrap();
        assert!(
            eth.is_live(ContractPurpose::CoordinationGame),
            "eth same-chain live"
        );
        assert!(
            eth.is_live(ContractPurpose::CrossChainGame),
            "eth cross-chain live"
        );
        assert!(!eth.is_live(ContractPurpose::ShillbotEscrow));
        assert!(eth.contract_for(ContractPurpose::ShillbotEscrow).is_none());
        // Base mainnet: same-chain + cross-chain game contracts live (2026-07-09 /
        // 2026-07-11); shillbot escrow stays None until its deploy.
        let base = entry(&ChainId::parse("eip155:8453").unwrap()).unwrap();
        assert!(
            base.is_live(ContractPurpose::CoordinationGame),
            "base same-chain live"
        );
        assert!(
            base.is_live(ContractPurpose::CrossChainGame),
            "base cross-chain live"
        );
        assert!(!base.is_live(ContractPurpose::ShillbotEscrow));
        // Base Sepolia (testnet) IS live for the game contracts — is_live true.
        let sepolia = entry(&ChainId::parse("eip155:84532").unwrap()).unwrap();
        assert!(sepolia.is_live(ContractPurpose::CrossChainGame));
        assert!(sepolia.is_live(ContractPurpose::CoordinationGame));
    }

    #[test]
    fn contract_for_resolves_by_purpose() {
        let base = ChainId::parse("eip155:84532").unwrap();
        // Base Sepolia hosts BOTH the cross-chain and same-chain contracts.
        assert_eq!(
            contract_for(&base, ContractPurpose::CrossChainGame),
            Some("0xd38b1fB07Bf64801bCBc3721937D6e2Ba6E5feb4")
        );
        assert_eq!(
            contract_for(&base, ContractPurpose::CoordinationGame),
            Some("0x042fE7202d208C9D79AdFd276da77b928C64514b")
        );
        // Solana has no same-chain EVM CoordinationGame, but has the cross-chain one.
        let sol = ChainId::parse("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1").unwrap();
        assert!(contract_for(&sol, ContractPurpose::CoordinationGame).is_none());
        assert!(contract_for(&sol, ContractPurpose::CrossChainGame).is_some());
        // Unregistered chain → None for any purpose.
        let unknown = ChainId::parse("eip155:999999").unwrap();
        assert!(contract_for(&unknown, ContractPurpose::CoordinationGame).is_none());
        assert!(contract_for(&unknown, ContractPurpose::ShillbotEscrow).is_none());
    }

    #[test]
    fn shillbot_escrow_resolves_on_base_sepolia_only() {
        // Deployed to Base Sepolia 2026-07-07 (S5 live demo); every other entry
        // stays None until its own deploy lands. Solana entries never carry an
        // EVM escrow address (the Solana leg is the shillbot program itself).
        for e in REGISTRY {
            let chain = ChainId::parse(e.chain_id).expect("valid CAIP-2");
            let resolved = contract_for(&chain, ContractPurpose::ShillbotEscrow);
            if e.chain_id == "eip155:84532" {
                assert_eq!(
                    resolved,
                    Some("0xaFe061778f9A76fCe7da4124dC89DAF8309E5F3c"),
                    "Base Sepolia must resolve to the deployed ShillbotEscrow"
                );
            } else {
                assert!(
                    resolved.is_none(),
                    "{}: shillbot escrow address must stay None until its deploy lands",
                    e.chain_id
                );
            }
        }
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
