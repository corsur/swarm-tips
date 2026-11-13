//! Stateless proof reference parser and EIP-191 `:sig:` cross-chain attribution
//! (`M1b` / `M2a`).

use crate::caip::{ChainId, Namespace};
use crate::onchain_action::crypto::{
    decode_0x_hex_exact, decode_base58_exact, decode_base64url_sig65, sha256,
};
use crate::onchain_action::{
    MAX_PROOF_CONTENT_ID_BYTES, MAX_RECEIPT_LOGS_SCANNED, MAX_SOLANA_IX_SCANNED,
};

/// Errors produced when parsing or verifying a submitted `OnChainAction` `content_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofParseError {
    Empty,
    TooLong {
        len: usize,
    },
    InvalidFormat,
    InvalidChainId,
    InvalidTxId,
    InvalidIndex,
    IndexOutOfBounds {
        index: u32,
        max: usize,
    },
    InvalidAttributionSig,
    AttributionSigNotAllowedOnSolana,
    #[cfg(feature = "cosign")]
    AttributionRecoveryFailed(crate::cosign::CosignError),
}

/// Parsed, canonicalized `content_id` proof reference:
/// `<caip2_ns>:<caip2_ref>:<tx_id>:<index>[:sig:<base64url_65B>]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnChainProofRef {
    pub chain_id: ChainId,
    /// Canonical transaction identifier (`0x` + 64 lowercase hex chars on `eip155`,
    /// or validated 64-byte base58 signature on `solana`).
    pub tx_id: String,
    /// Log index within the EVM receipt (`0..MAX_RECEIPT_LOGS_SCANNED`) or
    /// matched event/instruction index within the Solana transaction (`0..MAX_SOLANA_IX_SCANNED`).
    pub index: u32,
    /// Optional 65-byte `[r(32) || s(32) || v(1)]` EIP-191 signature binding an EVM
    /// referral address to the Solana `task.agent` pubkey (`M2a`).
    pub attribution_sig: Option<[u8; 65]>,
}

impl OnChainProofRef {
    /// Parse and validate a raw `content_id` string in pure CPU (`0` RPC / DB calls).
    pub fn parse(content_id: &str) -> Result<Self, ProofParseError> {
        let trimmed = content_id.trim();
        if trimmed.is_empty() {
            return Err(ProofParseError::Empty);
        }
        if trimmed.len() > MAX_PROOF_CONTENT_ID_BYTES {
            return Err(ProofParseError::TooLong { len: trimmed.len() });
        }

        let parts: Vec<&str> = trimmed.split(':').collect();
        if parts.len() != 4 && parts.len() != 6 {
            return Err(ProofParseError::InvalidFormat);
        }

        let caip2_str = format!("{}:{}", parts[0], parts[1]);
        let chain_id = ChainId::parse(&caip2_str).map_err(|_| ProofParseError::InvalidChainId)?;

        let index: u32 = parts[3]
            .parse()
            .map_err(|_| ProofParseError::InvalidIndex)?;
        let max_idx = match chain_id.namespace() {
            Namespace::Eip155 => MAX_RECEIPT_LOGS_SCANNED,
            Namespace::Solana => MAX_SOLANA_IX_SCANNED,
        };
        if (index as usize) >= max_idx {
            return Err(ProofParseError::IndexOutOfBounds {
                index,
                max: max_idx,
            });
        }

        let tx_id = match chain_id.namespace() {
            Namespace::Eip155 => {
                let raw_hash =
                    decode_0x_hex_exact::<32>(parts[2]).ok_or(ProofParseError::InvalidTxId)?;
                crate::onchain_action::crypto::encode_0x_hex(&raw_hash)
            }
            Namespace::Solana => {
                let _sig_bytes =
                    decode_base58_exact::<64>(parts[2]).ok_or(ProofParseError::InvalidTxId)?;
                parts[2].to_string()
            }
        };

        let attribution_sig = if parts.len() == 6 {
            if parts[4] != "sig" {
                return Err(ProofParseError::InvalidFormat);
            }
            if chain_id.namespace() != Namespace::Eip155 {
                return Err(ProofParseError::AttributionSigNotAllowedOnSolana);
            }
            let sig =
                decode_base64url_sig65(parts[5]).ok_or(ProofParseError::InvalidAttributionSig)?;
            Some(sig)
        } else {
            None
        };

        debug_assert!(!tx_id.is_empty());
        Ok(Self {
            chain_id,
            tx_id,
            index,
            attribution_sig,
        })
    }

    /// Per-event/instruction replay-lock key (`M1b`):
    /// `sha256("{caip2}:{canonical_target}:{canonical_tx_id}:{index}")`.
    ///
    /// For `eip155`, `canonical_target` is lowercase `0x` + 40 hex chars; for `solana`,
    /// `canonical_target` is exact base58 program ID (never lowercased!).
    pub fn replay_lock_id(&self, canonical_target: &str) -> [u8; 32] {
        let preimage = format!(
            "{}:{}:{}:{}",
            self.chain_id.as_str(),
            canonical_target,
            self.tx_id,
            self.index
        );
        sha256(preimage.as_bytes())
    }

    /// Transaction-wide replay-lock key used whenever a predicate evaluates
    /// transaction-level native value (`EvmFieldRef::TxValue` or `min_lamports > 0`),
    /// preventing a batched multi-call transaction from satisfying `N` tasks with `1x` value:
    /// `sha256("{caip2}:{canonical_target}:{canonical_tx_id}:tx")`.
    pub fn tx_replay_lock_id(&self, canonical_target: &str) -> [u8; 32] {
        let preimage = format!(
            "{}:{}:{}:tx",
            self.chain_id.as_str(),
            canonical_target,
            self.tx_id
        );
        sha256(preimage.as_bytes())
    }

    /// Select the sound replay-lock digest for `spec` (`tx`-scoped if `spec.uses_tx_wide_value()`,
    /// otherwise event/instruction-index-scoped).
    pub fn effective_replay_lock_id(
        &self,
        spec: &crate::onchain_action::OnChainActionSpec,
    ) -> [u8; 32] {
        let target = spec.canonical_target_address();
        if spec.uses_tx_wide_value() {
            self.tx_replay_lock_id(&target)
        } else {
            self.replay_lock_id(&target)
        }
    }

    /// Hex-encoded Firestore document ID for `consumed_onchain_proofs/{doc_id}`
    /// (per-event/instruction scope).
    pub fn replay_lock_doc_id(&self, canonical_target: &str) -> String {
        let digest = self.replay_lock_id(canonical_target);
        crate::onchain_action::crypto::encode_0x_hex(&digest)[2..].to_string()
    }

    /// Hex-encoded Firestore document ID for `consumed_onchain_proofs/{doc_id}`
    /// scoped automatically to `tx` when `spec.uses_tx_wide_value()` is true.
    pub fn effective_replay_lock_doc_id(
        &self,
        spec: &crate::onchain_action::OnChainActionSpec,
    ) -> String {
        let digest = self.effective_replay_lock_id(spec);
        crate::onchain_action::crypto::encode_0x_hex(&digest)[2..].to_string()
    }

    /// Recover the 20-byte EVM address bound to `agent_solana_b58` via the inline
    /// `:sig:` EIP-191 signature (`"swarm-attr-v1:<caip2_target>:<agent_solana_b58>"`).
    #[cfg(feature = "cosign")]
    pub fn recover_evm_attribution(
        &self,
        agent_solana_b58: &str,
    ) -> Result<Option<[u8; 20]>, ProofParseError> {
        let Some(sig) = &self.attribution_sig else {
            return Ok(None);
        };
        let _agent_pk = decode_base58_exact::<32>(agent_solana_b58)
            .ok_or(ProofParseError::InvalidAttributionSig)?;
        let msg = evm_attribution_message(&self.chain_id, agent_solana_b58);
        let digest = eip191_digest(msg.as_bytes());
        let recovered = crate::cosign::recover_address(&digest, sig)
            .map_err(ProofParseError::AttributionRecoveryFailed)?;
        Ok(Some(recovered))
    }
}

/// Canonical domain-separated attribution message signed by an agent's EVM key (`M2a`).
pub fn evm_attribution_message(target_chain: &ChainId, agent_solana_b58: &str) -> String {
    format!(
        "swarm-attr-v1:{}:{}",
        target_chain.as_str(),
        agent_solana_b58
    )
}

/// Compute EIP-191 `personal_sign` digest:
/// `keccak256("\x19Ethereum Signed Message:\n" || decimal_len(msg) || msg)`.
#[cfg(feature = "keccak")]
pub fn eip191_digest(message: &[u8]) -> [u8; 32] {
    let prefix = format!("\x19Ethereum Signed Message:\n{}", message.len());
    let mut buf = Vec::with_capacity(prefix.len().saturating_add(message.len()));
    buf.extend_from_slice(prefix.as_bytes());
    buf.extend_from_slice(message);
    crate::cert_schema::keccak256(&buf)
}

/// Produce the URL-safe unpadded base64 `:sig:` token for an EVM key attributing
/// conversions on `target_chain` to `agent_solana_b58`.
#[cfg(feature = "cosign")]
pub fn sign_evm_attribution(
    evm_secret_key: &[u8; 32],
    target_chain: &ChainId,
    agent_solana_b58: &str,
) -> Result<String, crate::cosign::CosignError> {
    let msg = evm_attribution_message(target_chain, agent_solana_b58);
    let digest = eip191_digest(msg.as_bytes());
    let sig = crate::cosign::sign_digest_eth(evm_secret_key, &digest)?;
    Ok(crate::onchain_action::crypto::encode_base64url_nopad(&sig))
}
