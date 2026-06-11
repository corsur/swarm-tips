//! Chain-agnostic transaction reference and status types — the seam
//! services dispatch through for submit/status (MCP `*_submit_tx`,
//! polling). Transaction BUILDING stays per-chain and concrete
//! (`game-chain`, `evm-chain`); these types only identify and track.

use crate::caip::ChainId;

/// A submitted transaction: a Solana signature or an EVM tx hash,
/// always paired with the chain it lives on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TxRef {
    pub chain: ChainId,
    /// Base58 signature (solana) or 0x-prefixed hash (eip155).
    pub id: String,
}

/// Coarse cross-chain transaction status. Per-chain finality nuance
/// (confirmed vs finalized, sequencer vs L1-posted) lives in the chain
/// registry's finality definitions, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum TxStatus {
    Pending,
    Confirmed,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_ref_carries_chain_context() {
        let chain = ChainId::parse("eip155:84532").expect("valid chain id");
        let tx = TxRef {
            chain: chain.clone(),
            id: "0xabc".to_owned(),
        };
        assert_eq!(tx.chain, chain);
        assert_ne!(TxStatus::Pending, TxStatus::Confirmed);
        assert_ne!(TxStatus::Confirmed, TxStatus::Failed);
    }
}
