//! Cross-chain (Solana ↔ EVM) discovery for MCP agents.
//!
//! The `xchain_supported_chains` tool exposes the chain registry's cross-chain
//! match configuration so an agent can discover which chains it can stake on
//! and at what amount before registering a wallet. This is the read-only entry
//! point to the cross-chain game; the stake-and-match tools (which pair a
//! Solana leg with an EVM leg via the game-api matchmaker and return unsigned
//! per-leg transactions) build on the same registry as their single source of
//! truth for addresses and stake amounts.
//!
//! Pure over `chain_registry` — no network, no state — so it is unit-tested
//! directly against the registry.

use serde_json::{json, Value};

/// One chain's agent-facing cross-chain match configuration. Base-unit amounts
/// are stringified: they are `u128` (wei) and can exceed JSON's safe-integer
/// range, so a string is the lossless wire form.
fn chain_json(e: &chain_registry::ChainEntry) -> Value {
    let namespace = chain_core::ChainId::parse(e.chain_id)
        .map(|c| c.namespace().as_str())
        .unwrap_or("unknown");
    json!({
        "chain_id": e.chain_id,
        "namespace": namespace,
        "display_name": e.display_name,
        "native_symbol": e.native_symbol,
        "native_decimals": e.native_decimals,
        "stake_base_units": e.stake_base_units.to_string(),
        "max_tranche_base_units": e.max_tranche_base_units.to_string(),
        "claim_window_secs": e.claim_window_secs,
        "game_contract": e.game_contract,
    })
}

/// The full `xchain_supported_chains` response: every registered chain plus a
/// plain-language description of how cross-chain matches resolve.
pub fn supported_chains_response() -> Value {
    let chains: Vec<Value> = chain_registry::all().map(chain_json).collect();
    json!({
        "chains": chains,
        "match_model": "A cross-chain match pairs a Solana leg (leg A) with an EVM leg \
            (leg B). Each player stakes their own chain's native coin into that chain's \
            game contract; the operator locks a float-pool tranche on each leg covering \
            the cross-chain counter-value of the opponent's stake. The match is settled \
            by a single certificate co-signed by both players' per-match session keys and \
            the operator, which each chain verifies independently and executes only its \
            own leg of — so a double-claim across chains is structurally impossible.",
        "settlement": "Instant path: a co-signed outcome certificate settles both legs. \
            Contested/timeout path: optimistic claims against the latest co-signed \
            checkpoint, with an equivocation-slash backstop.",
        "status": "testnet",
        "note": "Mainnet cross-chain routes are gated pending legal review and EVM \
            authority key rotation; only testnet chains (Solana devnet, Base Sepolia) \
            are live today.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_registered_chain_with_required_fields() {
        let resp = supported_chains_response();
        let chains = resp["chains"].as_array().expect("chains is an array");
        assert_eq!(chains.len(), chain_registry::all().count());

        for c in chains {
            // Every field the agent needs to decide whether to play must be present.
            assert!(c["chain_id"].is_string(), "chain_id missing");
            assert!(
                matches!(c["namespace"].as_str(), Some("solana") | Some("eip155")),
                "namespace must be a known CAIP namespace, got {:?}",
                c["namespace"]
            );
            assert!(c["native_symbol"].is_string(), "native_symbol missing");
            // Stake is stringified u128 and must parse back to a positive value.
            let stake: u128 = c["stake_base_units"]
                .as_str()
                .expect("stake_base_units is a string")
                .parse()
                .expect("stake_base_units parses as u128");
            assert!(stake > 0, "stake must be positive");
        }
    }

    #[test]
    fn base_sepolia_is_present_with_its_deployed_contract() {
        let resp = supported_chains_response();
        let chains = resp["chains"].as_array().unwrap();
        let base = chains
            .iter()
            .find(|c| c["chain_id"] == "eip155:84532")
            .expect("Base Sepolia registered");
        assert_eq!(base["namespace"], "eip155");
        assert_eq!(base["native_symbol"], "ETH");
        // The deployed CrossChainGame address must be surfaced to agents.
        assert!(
            base["game_contract"]
                .as_str()
                .unwrap_or("")
                .starts_with("0x"),
            "Base Sepolia must expose its deployed contract address"
        );
    }
}
