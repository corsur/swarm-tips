//! Canonical `OnChainActionSpec` declarative verification schema, RFC 8785 JCS
//! commitment (`Task.content_hash = sha256(spec_jcs)`), safety denylists, and
//! stateless cross-chain `:sig:` proof attribution (`ONCHAIN_ACTION_CONSENSUS_2026-11-10.md`).

pub mod crypto;
pub mod jcs;
pub mod proof;

use crate::caip::{ChainId, Namespace};
pub use crypto::{
    decode_0x_hex_bounded, decode_0x_hex_exact, decode_base58_exact, decode_base64url_sig65,
    encode_0x_hex, encode_base64url_nopad, sha256,
};
#[cfg(feature = "keccak")]
pub use proof::eip191_digest;
#[cfg(feature = "cosign")]
pub use proof::sign_evm_attribution;
pub use proof::{evm_attribution_message, OnChainProofRef, ProofParseError};

/// Current schema version for `OnChainActionSpec`.
pub const ONCHAIN_ACTION_SCHEMA_VERSION: u16 = 1;
/// Hard byte cap on serialized `OnChainActionSpec` JCS JSON (`4 KiB`).
pub const MAX_ONCHAIN_SPEC_BYTES: usize = 4_096;
/// Maximum number of field/slice predicates per spec (NASA Power-of-Ten loop bound).
pub const MAX_FIELD_PREDICATES: usize = 8;
/// Maximum 32-byte ABI word index inspectable in an EVM log `data` payload (`0..16`).
pub const MAX_EVM_DATA_WORDS: u8 = 16;
/// Maximum logs scanned in an EVM receipt (`0..64`).
pub const MAX_RECEIPT_LOGS_SCANNED: usize = 64;
/// Maximum instructions/CPIs scanned in a Solana transaction (`0..64`).
pub const MAX_SOLANA_IX_SCANNED: usize = 64;
/// Maximum end byte offset inspectable in a Solana `emit_cpi!` or ix payload (`1 KiB`).
pub const MAX_SOLANA_IX_DATA_SLICE_END: u16 = 1_024;
/// Streamed HTTP body byte ceiling per RPC response (`256 KiB`).
pub const MAX_RPC_RESPONSE_BYTES: usize = 262_144;
/// Maximum `content_id` string length for `PlatformType::OnChainAction` (`256` bytes).
pub const MAX_PROOF_CONTENT_ID_BYTES: usize = 256;
/// Minimum total campaign escrow on Solana (`0.005 SOL`) so client protocol-fee
/// cost exceeds verifier/crank settlement tx fees (`M2b`).
pub const MIN_CAMPAIGN_ESCROW_LAMPORTS: u64 = 5_000_000;
/// Anchor `emit_cpi!` self-CPI instruction tag (`0x1d9acb512ea545e4_u64.to_le_bytes()`,
/// matching `anchor_lang::event::EVENT_IX_TAG_LE`).
pub const ANCHOR_EVENT_IX_TAG_LE: [u8; 8] = [0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d];

/// Denylisted EVM `topic0` event signatures that represent token approvals or
/// ownership handovers rather than conversion/affiliate events (`M2a` safety rail).
const DENYLISTED_EVM_TOPIC0: &[&str] = &[
    // Approval(address,address,uint256) (ERC-20 / ERC-721)
    "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925",
    // ApprovalForAll(address,address,bool) (ERC-721 / ERC-1155)
    "0x17307eab39ab6107e8899845ad3d59bd9653f200f220920489ca2b5937696c31",
    // OwnershipTransferred(address,address) (Ownable)
    "0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0",
];

/// Denylisted Solana system / token programs for raw `OnChainAction` targets
/// (`Assign`, `Approve`, `SetAuthority` wallet-drainer prevention).
const DENYLISTED_SOLANA_PROGRAMS: &[&str] = &[
    "11111111111111111111111111111111",
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
];

/// Execution mode for `PlatformType::OnChainAction`.
///
/// Only `AffiliateEvent` is permitted in v1 (`M2a`): worker agents never sign
/// blind calldata to unverified campaign contracts; they promote/drive conversions
/// and submit finalized receipt/tx proofs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ActionMode {
    AffiliateEvent,
}

/// Bounded comparison operators over 32-byte BE words (or LE `u64` slices on Solana).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PredicateOp {
    Eq,
    Ne,
    Gte,
}

impl PredicateOp {
    /// Evaluate `lhs <op> rhs` in constant-size big-endian unsigned integer order.
    pub fn eval_be32(self, lhs: &[u8; 32], rhs: &[u8; 32]) -> bool {
        match self {
            Self::Eq => lhs == rhs,
            Self::Ne => lhs != rhs,
            Self::Gte => lhs >= rhs,
        }
    }

    /// Evaluate `lhs <op> rhs` for unsigned 64-bit values.
    pub fn eval_u64(self, lhs: u64, rhs: u64) -> bool {
        match self {
            Self::Eq => lhs == rhs,
            Self::Ne => lhs != rhs,
            Self::Gte => lhs >= rhs,
        }
    }
}

/// Where the claimant's referral/affiliate attribution appears inside an EVM log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "source", rename_all = "snake_case"))]
pub enum EvmAttributionSource {
    /// Indexed event parameter `topics[1..=3]`.
    Topic { index: u8 },
    /// Unindexed 32-byte ABI word `data[word_index * 32 .. (word_index + 1) * 32]` (`0..16`).
    DataWord { word_index: u8 },
}

/// Reference to an EVM transaction or receipt-log word for predicates and `actor_field`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "field", rename_all = "snake_case"))]
pub enum EvmFieldRef {
    /// Outer transaction sender `tx.from` (left-padded with 12 zero bytes to 32 bytes).
    TxFrom,
    /// Outer transaction `tx.value` in wei (big-endian 32-byte `uint256`).
    /// Requires `require_direct_call == true` (`tx.to == target_contract`) per `M2b`.
    TxValue,
    /// Indexed event topic `topics[1..=3]`.
    Topic { index: u8 },
    /// Unindexed 32-byte ABI word `0..16`.
    DataWord { word_index: u8 },
}

/// One bounded predicate over an EVM transaction/log word.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EvmFieldPredicate {
    pub field: EvmFieldRef,
    pub op: PredicateOp,
    /// `0x` + 64 hex chars (32-byte big-endian word).
    pub value_hex: String,
}

/// Declarative EVM receipt-log predicate (`eth_getTransactionReceipt` + finalized block).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EvmLogPredicate {
    /// `0x` + 40 hex chars contract address (`log.address == target_contract`).
    pub target_contract: String,
    /// `0x` + 64 hex chars `keccak256(EventSignature)` (`log.topics[0] == topic0`).
    pub topic0: String,
    /// Optional human-readable Solidity event signature (e.g. `Sale(address,address,uint256)`).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub event_signature: Option<String>,
    /// Mandatory worker attribution field (`M1` / `M2a`).
    pub agent_attribution: EvmAttributionSource,
    /// Mandatory end-buyer / counterparty wallet field (`actor != agent && actor != client`).
    pub actor_field: EvmFieldRef,
    /// If `true`, requires outer `tx.to == Some(target_contract)`. Mandatory when `TxValue` is used.
    #[cfg_attr(feature = "serde", serde(default))]
    pub require_direct_call: bool,
    /// Up to [`MAX_FIELD_PREDICATES`] word comparisons.
    #[cfg_attr(feature = "serde", serde(default))]
    pub field_predicates: Vec<EvmFieldPredicate>,
}

/// How a Solana target program's conversion action is matched (`M2c` — never `meta.logMessages`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "mode", rename_all = "snake_case"))]
pub enum SolanaEventMatcher {
    /// Anchor `emit_cpi!` self-CPI (`PDA(["__event_authority"], program_id)` + `ANCHOR_EVENT_IX_TAG_LE`
    /// + 8-byte `event_discriminator_hex`).
    AnchorEmitCpi { event_discriminator_hex: String },
    /// Direct top-level or verified inner CPI instruction data prefix (`1..=8` bytes).
    InstructionData { ix_discriminator_hex: String },
}

/// Where the claimant's Solana pubkey appears in a matched Solana instruction / `emit_cpi!` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "source", rename_all = "snake_case"))]
pub enum SolanaAttributionSource {
    /// Account index `0..32` in the matched instruction's account list.
    AccountIndex { index: u8 },
    /// 32-byte pubkey at `byte_offset` inside the event payload (after 16B `emit_cpi!` header)
    /// or instruction data.
    PayloadPubkey { byte_offset: u16 },
}

/// Word encoding for [`SolanaSlicePredicate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SolanaSliceEncoding {
    /// 32-byte big-endian slice (`value_hex` = `0x` + 64 hex chars).
    Be256,
    /// 8-byte little-endian `u64` slice (`value_hex` = `0x` + 16 hex chars big-endian `u64` value).
    LeU64,
}

/// Bounded slice predicate on a matched Solana `emit_cpi!` payload or instruction data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SolanaSlicePredicate {
    pub byte_offset: u16,
    pub encoding: SolanaSliceEncoding,
    pub op: PredicateOp,
    pub value_hex: String,
}

/// Declarative Solana program predicate (`M2c`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SolanaProgramPredicate {
    /// Base58 32-byte target program ID.
    pub program_id: String,
    pub matcher: SolanaEventMatcher,
    pub agent_attribution: SolanaAttributionSource,
    /// Account index (`0..32`) of the end-buyer / counterparty (`actor != agent && actor != client`).
    pub actor_account_index: u8,
    /// Optional minimum native lamport balance delta paid by `actor_account_index` (`0` if N/A).
    #[cfg_attr(feature = "serde", serde(default))]
    pub min_lamports: u64,
    #[cfg_attr(feature = "serde", serde(default))]
    pub field_predicates: Vec<SolanaSlicePredicate>,
}

/// Chain-family predicate matching `target_chain`'s CAIP-2 namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum TargetPredicate {
    EvmLog(EvmLogPredicate),
    SolanaIx(SolanaProgramPredicate),
}

/// Top-level immutable specification committed into `Task.content_hash = sha256(to_jcs_bytes())`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OnChainActionSpec {
    pub schema_version: u16,
    /// CAIP-2 chain ID from `chain_registry::VERIFICATION_TARGETS` (e.g. `"eip155:56"`).
    pub target_chain: String,
    pub action_mode: ActionMode,
    pub predicate: TargetPredicate,
}

/// Validation failure reasons for [`OnChainActionSpec::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecValidationError {
    UnsupportedSchemaVersion(u16),
    InvalidTargetChain,
    ChainPredicateNamespaceMismatch,
    InvalidTargetContract,
    InvalidTopic0,
    DenylistedTopic0,
    InvalidEventSignature,
    InvalidTopicIndex(u8),
    InvalidDataWordIndex(u8),
    InvalidActorField,
    ActorEqualsAttributionField,
    TxValueRequiresDirectCall,
    TooManyPredicates(usize),
    InvalidPredicateValueHex,
    InvalidProgramId,
    DenylistedProgramId,
    InvalidDiscriminatorHex,
    InvalidAccountIndex(u8),
    InvalidSliceBounds(u16),
    SpecTooLarge(usize),
}

impl OnChainActionSpec {
    /// Validate all structural bounds, CAIP-2 namespace parity, safety denylists,
    /// and NASA Power-of-Ten caps.
    pub fn validate(&self) -> Result<(), SpecValidationError> {
        if self.schema_version != ONCHAIN_ACTION_SCHEMA_VERSION {
            return Err(SpecValidationError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        let chain = ChainId::parse(&self.target_chain)
            .map_err(|_| SpecValidationError::InvalidTargetChain)?;

        match (chain.namespace(), &self.predicate) {
            (Namespace::Eip155, TargetPredicate::EvmLog(p)) => validate_evm_predicate(p)?,
            (Namespace::Solana, TargetPredicate::SolanaIx(p)) => validate_solana_predicate(p)?,
            _ => return Err(SpecValidationError::ChainPredicateNamespaceMismatch),
        }

        let jcs_len = self.to_jcs_string().len();
        if jcs_len > MAX_ONCHAIN_SPEC_BYTES {
            return Err(SpecValidationError::SpecTooLarge(jcs_len));
        }
        Ok(())
    }

    /// Canonical target address/program string used in `OnChainProofRef::replay_lock_id`
    /// (`0x` + lowercase 40-hex for EVM; exact base58 program ID for Solana).
    pub fn canonical_target_address(&self) -> String {
        match &self.predicate {
            TargetPredicate::EvmLog(p) => p.target_contract.to_ascii_lowercase(),
            TargetPredicate::SolanaIx(p) => p.program_id.clone(),
        }
    }

    /// Serialize into canonical RFC 8785 JCS UTF-8 string.
    pub fn to_jcs_string(&self) -> String {
        jcs::serialize_spec_jcs(self)
    }

    /// Serialize into canonical RFC 8785 JCS UTF-8 bytes.
    pub fn to_jcs_bytes(&self) -> Vec<u8> {
        self.to_jcs_string().into_bytes()
    }

    /// Compute the 32-byte `Task.content_hash` commitment (`sha256(to_jcs_bytes())`).
    pub fn spec_commitment_sha256(&self) -> [u8; 32] {
        sha256(&self.to_jcs_bytes())
    }
}

fn validate_evm_predicate(p: &EvmLogPredicate) -> Result<(), SpecValidationError> {
    let contract_bytes = decode_0x_hex_exact::<20>(&p.target_contract)
        .ok_or(SpecValidationError::InvalidTargetContract)?;
    if contract_bytes == [0u8; 20] {
        return Err(SpecValidationError::InvalidTargetContract);
    }

    let topic0_bytes =
        decode_0x_hex_exact::<32>(&p.topic0).ok_or(SpecValidationError::InvalidTopic0)?;
    if topic0_bytes == [0u8; 32] {
        return Err(SpecValidationError::InvalidTopic0);
    }
    let topic0_lower = p.topic0.to_ascii_lowercase();
    if DENYLISTED_EVM_TOPIC0.contains(&topic0_lower.as_str()) {
        return Err(SpecValidationError::DenylistedTopic0);
    }

    if let Some(sig) = &p.event_signature {
        if sig.is_empty() || sig.len() > 256 || !sig.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
            return Err(SpecValidationError::InvalidEventSignature);
        }
    }

    match p.agent_attribution {
        EvmAttributionSource::Topic { index } if !(1..=3).contains(&index) => {
            return Err(SpecValidationError::InvalidTopicIndex(index));
        }
        EvmAttributionSource::DataWord { word_index } if word_index >= MAX_EVM_DATA_WORDS => {
            return Err(SpecValidationError::InvalidDataWordIndex(word_index));
        }
        _ => {}
    }

    validate_evm_actor_field(&p.actor_field, &p.agent_attribution)?;

    if p.field_predicates.len() > MAX_FIELD_PREDICATES {
        return Err(SpecValidationError::TooManyPredicates(
            p.field_predicates.len(),
        ));
    }

    let mut uses_tx_value = false;
    for fp in &p.field_predicates {
        match fp.field {
            EvmFieldRef::TxFrom => {}
            EvmFieldRef::TxValue => uses_tx_value = true,
            EvmFieldRef::Topic { index } if !(1..=3).contains(&index) => {
                return Err(SpecValidationError::InvalidTopicIndex(index));
            }
            EvmFieldRef::DataWord { word_index } if word_index >= MAX_EVM_DATA_WORDS => {
                return Err(SpecValidationError::InvalidDataWordIndex(word_index));
            }
            EvmFieldRef::Topic { .. } | EvmFieldRef::DataWord { .. } => {}
        }
        if decode_0x_hex_exact::<32>(&fp.value_hex).is_none() {
            return Err(SpecValidationError::InvalidPredicateValueHex);
        }
    }

    if uses_tx_value && !p.require_direct_call {
        return Err(SpecValidationError::TxValueRequiresDirectCall);
    }
    Ok(())
}

fn validate_evm_actor_field(
    actor: &EvmFieldRef,
    attr: &EvmAttributionSource,
) -> Result<(), SpecValidationError> {
    match *actor {
        EvmFieldRef::TxValue => return Err(SpecValidationError::InvalidActorField),
        EvmFieldRef::TxFrom => {}
        EvmFieldRef::Topic { index } => {
            if !(1..=3).contains(&index) {
                return Err(SpecValidationError::InvalidTopicIndex(index));
            }
            if matches!(*attr, EvmAttributionSource::Topic { index: a } if a == index) {
                return Err(SpecValidationError::ActorEqualsAttributionField);
            }
        }
        EvmFieldRef::DataWord { word_index } => {
            if word_index >= MAX_EVM_DATA_WORDS {
                return Err(SpecValidationError::InvalidDataWordIndex(word_index));
            }
            if matches!(*attr, EvmAttributionSource::DataWord { word_index: a } if a == word_index)
            {
                return Err(SpecValidationError::ActorEqualsAttributionField);
            }
        }
    }
    Ok(())
}

fn validate_solana_predicate(p: &SolanaProgramPredicate) -> Result<(), SpecValidationError> {
    let prog_bytes =
        decode_base58_exact::<32>(&p.program_id).ok_or(SpecValidationError::InvalidProgramId)?;
    if prog_bytes == [0u8; 32] {
        return Err(SpecValidationError::InvalidProgramId);
    }
    if DENYLISTED_SOLANA_PROGRAMS.contains(&p.program_id.as_str()) {
        return Err(SpecValidationError::DenylistedProgramId);
    }

    match &p.matcher {
        SolanaEventMatcher::AnchorEmitCpi {
            event_discriminator_hex,
        } => {
            if decode_0x_hex_exact::<8>(event_discriminator_hex).is_none() {
                return Err(SpecValidationError::InvalidDiscriminatorHex);
            }
        }
        SolanaEventMatcher::InstructionData {
            ix_discriminator_hex,
        } => {
            if decode_0x_hex_bounded(ix_discriminator_hex, 8).is_none() {
                return Err(SpecValidationError::InvalidDiscriminatorHex);
            }
        }
    }

    if p.actor_account_index >= 32 {
        return Err(SpecValidationError::InvalidAccountIndex(
            p.actor_account_index,
        ));
    }

    match p.agent_attribution {
        SolanaAttributionSource::AccountIndex { index } => {
            if index >= 32 {
                return Err(SpecValidationError::InvalidAccountIndex(index));
            }
            if index == p.actor_account_index {
                return Err(SpecValidationError::ActorEqualsAttributionField);
            }
        }
        SolanaAttributionSource::PayloadPubkey { byte_offset } => {
            if byte_offset.saturating_add(32) > MAX_SOLANA_IX_DATA_SLICE_END {
                return Err(SpecValidationError::InvalidSliceBounds(byte_offset));
            }
        }
    }

    if p.field_predicates.len() > MAX_FIELD_PREDICATES {
        return Err(SpecValidationError::TooManyPredicates(
            p.field_predicates.len(),
        ));
    }

    for fp in &p.field_predicates {
        let width: u16 = match fp.encoding {
            SolanaSliceEncoding::Be256 => {
                if decode_0x_hex_exact::<32>(&fp.value_hex).is_none() {
                    return Err(SpecValidationError::InvalidPredicateValueHex);
                }
                32
            }
            SolanaSliceEncoding::LeU64 => {
                if decode_0x_hex_exact::<8>(&fp.value_hex).is_none() {
                    return Err(SpecValidationError::InvalidPredicateValueHex);
                }
                8
            }
        };
        if fp.byte_offset.saturating_add(width) > MAX_SOLANA_IX_DATA_SLICE_END {
            return Err(SpecValidationError::InvalidSliceBounds(fp.byte_offset));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bsc_affiliate_spec() -> OnChainActionSpec {
        // Mirrors external agent HCqPm...'s Nov 1 BSC NFT affiliate use case:
        // 0.005 BNB (5_000_000_000_000_000 wei = 0x0011c37937e08000) mint on
        // 0xbe9a615678b2f3f06196b2de3f331c2c8c149d56 with indexed buyer & referrer.
        OnChainActionSpec {
            schema_version: ONCHAIN_ACTION_SCHEMA_VERSION,
            target_chain: "eip155:56".to_string(),
            action_mode: ActionMode::AffiliateEvent,
            predicate: TargetPredicate::EvmLog(EvmLogPredicate {
                target_contract: "0xBe9A615678B2F3F06196B2dE3F331c2C8c149D56".to_string(),
                topic0: "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
                    .to_string(),
                event_signature: Some("AffiliateMint(address,address,uint256)".to_string()),
                agent_attribution: EvmAttributionSource::Topic { index: 2 },
                actor_field: EvmFieldRef::Topic { index: 1 },
                require_direct_call: true,
                field_predicates: vec![EvmFieldPredicate {
                    field: EvmFieldRef::TxValue,
                    op: PredicateOp::Gte,
                    value_hex: "0x0000000000000000000000000000000000000000000000000011c37937e08000"
                        .to_string(),
                }],
            }),
        }
    }

    #[test]
    fn sha256_matches_nist_fips_180_4_vectors() {
        assert_eq!(
            encode_0x_hex(&sha256(b"")),
            "0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            encode_0x_hex(&sha256(b"abc")),
            "0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            encode_0x_hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "0x248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn bsc_affiliate_spec_validates_and_normalizes_hex_case_in_jcs() {
        let mixed = sample_bsc_affiliate_spec();
        mixed.validate().expect("BSC affiliate spec valid");

        let mut lower = mixed.clone();
        if let TargetPredicate::EvmLog(ref mut p) = lower.predicate {
            p.target_contract = p.target_contract.to_ascii_lowercase();
        }
        assert_eq!(mixed.to_jcs_string(), lower.to_jcs_string());
        assert_eq!(
            mixed.spec_commitment_sha256(),
            lower.spec_commitment_sha256()
        );
        assert!(mixed
            .to_jcs_string()
            .starts_with("{\"action_mode\":\"affiliate_event\",\"predicate\":{\"actor_field\":"));
    }

    #[test]
    fn rejects_denylisted_approval_topic0_and_system_program() {
        let mut spec = sample_bsc_affiliate_spec();
        if let TargetPredicate::EvmLog(ref mut p) = spec.predicate {
            p.topic0 =
                "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925".to_string();
        }
        assert_eq!(spec.validate(), Err(SpecValidationError::DenylistedTopic0));

        let sol_bad = OnChainActionSpec {
            schema_version: ONCHAIN_ACTION_SCHEMA_VERSION,
            target_chain: "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp".to_string(),
            action_mode: ActionMode::AffiliateEvent,
            predicate: TargetPredicate::SolanaIx(SolanaProgramPredicate {
                program_id: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
                matcher: SolanaEventMatcher::InstructionData {
                    ix_discriminator_hex: "0x04".to_string(),
                },
                agent_attribution: SolanaAttributionSource::AccountIndex { index: 1 },
                actor_account_index: 0,
                min_lamports: 0,
                field_predicates: vec![],
            }),
        };
        assert_eq!(
            sol_bad.validate(),
            Err(SpecValidationError::DenylistedProgramId)
        );
    }

    #[test]
    fn rejects_tx_value_without_require_direct_call_and_actor_attribution_collision() {
        let mut spec = sample_bsc_affiliate_spec();
        if let TargetPredicate::EvmLog(ref mut p) = spec.predicate {
            p.require_direct_call = false;
        }
        assert_eq!(
            spec.validate(),
            Err(SpecValidationError::TxValueRequiresDirectCall)
        );

        let mut collision = sample_bsc_affiliate_spec();
        if let TargetPredicate::EvmLog(ref mut p) = collision.predicate {
            p.actor_field = EvmFieldRef::Topic { index: 2 };
        }
        assert_eq!(
            collision.validate(),
            Err(SpecValidationError::ActorEqualsAttributionField)
        );
    }

    #[test]
    fn solana_anchor_emit_cpi_spec_validates_and_commits() {
        let spec = OnChainActionSpec {
            schema_version: ONCHAIN_ACTION_SCHEMA_VERSION,
            target_chain: "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1".to_string(),
            action_mode: ActionMode::AffiliateEvent,
            predicate: TargetPredicate::SolanaIx(SolanaProgramPredicate {
                program_id: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P".to_string(),
                matcher: SolanaEventMatcher::AnchorEmitCpi {
                    event_discriminator_hex: "0x0102030405060708".to_string(),
                },
                agent_attribution: SolanaAttributionSource::PayloadPubkey { byte_offset: 0 },
                actor_account_index: 0,
                min_lamports: 10_000_000,
                field_predicates: vec![SolanaSlicePredicate {
                    byte_offset: 32,
                    encoding: SolanaSliceEncoding::LeU64,
                    op: PredicateOp::Gte,
                    value_hex: "0x0000000000989680".to_string(),
                }],
            }),
        };
        spec.validate().expect("Solana emit_cpi spec valid");
        assert_ne!(spec.spec_commitment_sha256(), [0u8; 32]);
    }

    #[cfg(feature = "cosign")]
    #[test]
    fn proof_ref_parses_and_recovers_stateless_eip191_attribution_sig() {
        let mut sk = [0u8; 32];
        sk[31] = 0x55;
        let expected_evm = crate::cosign::eth_address(&sk).unwrap();
        let chain = ChainId::parse("eip155:56").unwrap();
        let agent_b58 = "HCqPmPJM3NVtS6YrmVaTZYnuP8XZ2PFgQZKT94VK1LkS";

        let sig_b64 = sign_evm_attribution(&sk, &chain, agent_b58).unwrap();
        let content_id = format!(
            "eip155:56:0xABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789:2:sig:{sig_b64}"
        );
        assert!(
            content_id.len() <= MAX_PROOF_CONTENT_ID_BYTES,
            "content_id len {} > 256",
            content_id.len()
        );

        let parsed = OnChainProofRef::parse(&content_id).expect("valid proof ref");
        assert_eq!(parsed.chain_id.as_str(), "eip155:56");
        assert_eq!(
            parsed.tx_id,
            "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        );
        assert_eq!(parsed.index, 2);

        let recovered = parsed
            .recover_evm_attribution(agent_b58)
            .expect("recovery ok")
            .expect("sig present");
        assert_eq!(recovered, expected_evm);

        // Submitting the same :sig: under a DIFFERENT Solana agent pubkey recovers a
        // random address != expected_evm (defeating mempool frontrunning!).
        let frontrunner_b58 = "5vsGoTRoc5j1a2fKszyZ7y28G6ggmu87YobpwzuXsMhu";
        let wrong = parsed
            .recover_evm_attribution(frontrunner_b58)
            .expect("recovery ok")
            .expect("sig present");
        assert_ne!(wrong, expected_evm);
    }

    #[test]
    fn anchor_event_ix_tag_le_matches_anchor_lang_convention() {
        let digest = sha256(b"anchor:event");
        let mut be8 = [0u8; 8];
        be8.copy_from_slice(&digest[..8]);
        let event_ix_tag_u64 = u64::from_be_bytes(be8);
        assert_eq!(event_ix_tag_u64, 0x1d9a_cb51_2ea5_45e4);
        assert_eq!(ANCHOR_EVENT_IX_TAG_LE, event_ix_tag_u64.to_le_bytes());
    }
}
