//! Canonical `OnChainActionSpec` declarative verification schema, RFC 8785 JCS
//! commitment (`Task.content_hash = sha256(spec_jcs)`), safety denylists, and
//! stateless cross-chain `:sig:` proof attribution (`ONCHAIN_ACTION_CONSENSUS_2026-11-10.md`).

pub mod crypto;
pub mod jcs;
pub mod proof;

use crate::caip::{ChainId, Namespace};
pub use crypto::{
    decode_0x_hex_bounded, decode_0x_hex_exact, decode_base58_exact, decode_base64url_sig65,
    decode_sig65, encode_0x_hex, encode_base64url_nopad, sha256,
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
/// Maximum logs scanned in an EVM receipt (`0..256`).
pub const MAX_RECEIPT_LOGS_SCANNED: usize = 256;
/// Maximum instructions/CPIs scanned in a Solana transaction (`0..256`).
pub const MAX_SOLANA_IX_SCANNED: usize = 256;
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

/// Maximum integer representable losslessly in IEEE-754 double-precision JSON (`2^53 - 1`),
/// ensuring byte-identical RFC 8785 JCS canonicalization across Rust and JS/TS runtimes.
pub const MAX_SAFE_JS_INTEGER_U64: u64 = 9_007_199_254_740_991;
pub const MAX_SAFE_JS_INTEGER_I64: i64 = 9_007_199_254_740_991;

/// Denylisted EVM target contracts (shared approval routers, smart-account factories,
/// and ERC-4337 entrypoints where conversion attribution can be spoofed or abused).
const DENYLISTED_EVM_CONTRACTS: &[&str] = &[
    // Uniswap Permit2 (canonical deterministic address across EVM chains)
    "0x000000000022d473030f116ddee9f6b43ac78ba3",
    // Safe v1.3.0 / v1.4.1 singletons & proxy factories
    "0xd9db270c1b5e3bd161e8c8503c55ceabee709552",
    "0x3e5c63644e683549055b9be8653de26e0b4cd36e",
    "0x41675c099f32341bf84bfc5382af534df5c7461a",
    "0x29fcb43b46531bca003ddc8fcb67ffe91900c762",
    "0xa6b71e26c5e0845f74c812102ca7114b6a896ab2",
    "0x4e1dcf7ad4e460cfd30791ccc4f9c8a4f820ec67",
    // ERC-4337 EntryPoint v0.6 & v0.7
    "0x5ff137d4b0fdcd49dca30c7cf57e578a026d2789",
    "0x0000000071727de22e5e9d8baf0edac6f37da032",
];

/// Denylisted EVM `topic0` event signatures that represent token approvals,
/// permit signatures, or wallet ownership/module handovers (`M2a` safety rail).
const DENYLISTED_EVM_TOPIC0: &[&str] = &[
    // Approval(address,address,uint256) (ERC-20 / ERC-721 / WETH9)
    "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925",
    // ApprovalForAll(address,address,bool) (ERC-721 / ERC-1155)
    "0x17307eab39ab6107e8899845ad3d59bd9653f200f220920489ca2b5937696c31",
    // OwnershipTransferred(address,address) (Ownable)
    "0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0",
    // Permit2 Approval(address,address,address,uint160,uint48)
    "0xcd3829a3813dc3cdd188fd3d01dcf3268c16be2fdd2dd21d0665418816e46062",
    // Permit2 Permit(address,address,address,uint160,uint48,uint48)
    "0xc6a377bfc4eb120024a8ac08eef205be16b817020812c73223e81d1bdb9708ec",
    // Safe AddedOwner(address) & EnabledModule(address)
    "0x9465fa0c962cc76958e6373a993326400c1c94f8be2fe3a952adfa7f60b2ea26",
    "0xecdf3a3effea5783a3c4c2140e677577666428d44ef9d474a0b3a4c26123fd77",
];

/// Denylisted Solana system / token / stake / loader programs for raw `OnChainAction`
/// targets (`Assign`, `Approve`, `SetAuthority`, `Authorize` wallet-drainer prevention).
const DENYLISTED_SOLANA_PROGRAMS: &[&str] = &[
    "11111111111111111111111111111111",
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
    "Stake11111111111111111111111111111111111111",
    "BPFLoaderUpgradeab1e11111111111111111111111",
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
#[cfg_attr(
    feature = "serde",
    serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum EvmAttributionSource {
    /// Indexed event parameter `topics[1..=3]`.
    Topic { index: u8 },
    /// Unindexed 32-byte ABI word `data[word_index * 32 .. (word_index + 1) * 32]` (`0..16`).
    DataWord { word_index: u8 },
}

/// Reference to an EVM transaction or receipt-log word for predicates and `actor_field`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "field", rename_all = "snake_case", deny_unknown_fields)
)]
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
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct EvmFieldPredicate {
    pub field: EvmFieldRef,
    pub op: PredicateOp,
    /// `0x` + 64 hex chars (32-byte big-endian word); also accepts unpadded `0x` hex
    /// or base-10 unsigned integer strings via [`normalize_uint256_value_hex`].
    pub value_hex: String,
}

/// Declarative EVM receipt-log predicate (`eth_getTransactionReceipt` + finalized block).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
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
#[cfg_attr(
    feature = "serde",
    serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)
)]
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
#[cfg_attr(
    feature = "serde",
    serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)
)]
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
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SolanaSlicePredicate {
    pub byte_offset: u16,
    pub encoding: SolanaSliceEncoding,
    pub op: PredicateOp,
    pub value_hex: String,
}

/// Declarative Solana program predicate (`M2c`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SolanaProgramPredicate {
    /// Base58 32-byte target program ID.
    pub program_id: String,
    pub matcher: SolanaEventMatcher,
    pub agent_attribution: SolanaAttributionSource,
    /// Account index (`0..32`) of the end-buyer / counterparty (`actor != agent && actor != client`).
    pub actor_account_index: u8,
    /// Optional minimum native lamport balance delta (`0` if N/A; capped at `2^53 - 1` for JCS parity).
    #[cfg_attr(feature = "serde", serde(default))]
    pub min_lamports: u64,
    /// Optional recipient/vault account index (`0..32`) that must gain `>= min_lamports`
    /// (`postBalances[idx] - preBalances[idx] >= min_lamports`), preventing rent-reclaim
    /// side-instruction balance spoofing.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub recipient_account_index: Option<u8>,
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
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct OnChainActionSpec {
    pub schema_version: u16,
    /// CAIP-2 chain ID from `chain_registry::VERIFICATION_TARGETS` (e.g. `"eip155:56"`).
    pub target_chain: String,
    pub action_mode: ActionMode,
    /// Optional campaign-wide minimum Unix timestamp floor (`block.timestamp >= min_block_timestamp`).
    /// Verifiers always enforce `max(onchain_task.created_at, min_block_timestamp)`.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub min_block_timestamp: Option<i64>,
    pub predicate: TargetPredicate,
}

/// Validation failure reasons for [`OnChainActionSpec::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecValidationError {
    UnsupportedSchemaVersion(u16),
    InvalidTargetChain,
    ChainPredicateNamespaceMismatch,
    InvalidMinBlockTimestamp(i64),
    InvalidTargetContract,
    DenylistedTargetContract,
    InvalidTopic0,
    DenylistedTopic0,
    InvalidEventSignature,
    InvalidTopicIndex(u8),
    InvalidDataWordIndex(u8),
    InvalidActorField,
    ActorEqualsAttributionField,
    RecipientEqualsActorOrAttribution,
    TxValueRequiresDirectCall,
    TooManyPredicates(usize),
    InvalidPredicateValueHex,
    InvalidProgramId,
    DenylistedProgramId,
    InvalidDiscriminatorHex,
    InvalidAccountIndex(u8),
    MinLamportsExceedsSafeJsInteger(u64),
    InvalidSliceBounds(u16),
    SpecTooLarge(usize),
}

fn is_valid_solidity_ident(s: &str) -> bool {
    let mut bytes = s.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_' || first == b'$') {
        return false;
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
}

fn normalize_elementary_abi_type(base: &str) -> Option<&str> {
    if base.is_empty() {
        return None;
    }
    match base {
        "indexed" | "anonymous" | "event" | "memory" | "calldata" | "storage" | "returns" => None,
        "uint" => Some("uint256"),
        "int" => Some("int256"),
        "ufixed" => Some("ufixed128x18"),
        "fixed" => Some("fixed128x18"),
        other => {
            let mut chars = other.bytes();
            let first = chars.next()?;
            if !first.is_ascii_lowercase() {
                return None;
            }
            if !chars.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) {
                return None;
            }
            Some(other)
        }
    }
}

/// Parse zero or more `[...]` array suffixes starting at the beginning of `rest`,
/// returning `(canonical_brackets, remaining_tail)`.
fn consume_array_suffixes(mut rest: &str) -> Result<(String, &str), SpecValidationError> {
    let mut brackets = String::new();
    loop {
        let trimmed = rest.trim_start();
        let Some(after_open) = trimmed.strip_prefix('[') else {
            return Ok((brackets, trimmed));
        };
        let close_pos = after_open
            .find(']')
            .ok_or(SpecValidationError::InvalidEventSignature)?;
        let len_str = after_open[..close_pos].trim();
        if !len_str.is_empty() && !len_str.bytes().all(|b| b.is_ascii_digit()) {
            return Err(SpecValidationError::InvalidEventSignature);
        }
        brackets.push('[');
        brackets.push_str(len_str);
        brackets.push(']');
        rest = &after_open[close_pos.saturating_add(1)..];
    }
}

fn validate_trailing_param_tokens(tail: &str) -> Result<(), SpecValidationError> {
    let mut seen_name = false;
    for word in tail.split_ascii_whitespace() {
        match word {
            "indexed" | "anonymous" | "memory" | "calldata" | "storage" => {}
            name => {
                if seen_name || !is_valid_solidity_ident(name) {
                    return Err(SpecValidationError::InvalidEventSignature);
                }
                seen_name = true;
            }
        }
    }
    Ok(())
}

fn find_matching_close_paren(s: &str) -> Result<usize, SpecValidationError> {
    let mut depth = 0usize;
    for (idx, b) in s.bytes().enumerate() {
        match b {
            b'(' => depth = depth.saturating_add(1),
            b')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(SpecValidationError::InvalidEventSignature)?;
                if depth == 0 {
                    return Ok(idx);
                }
            }
            _ => {}
        }
    }
    Err(SpecValidationError::InvalidEventSignature)
}

fn canonicalize_abi_param_list(inner: &str, depth: usize) -> Result<String, SpecValidationError> {
    if depth > 8 {
        return Err(SpecValidationError::InvalidEventSignature);
    }
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        if depth == 0 {
            return Ok(String::new());
        }
        return Err(SpecValidationError::InvalidEventSignature);
    }

    let mut segments: Vec<&str> = Vec::new();
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut seg_start = 0usize;
    for (idx, b) in trimmed.bytes().enumerate() {
        match b {
            b'(' => paren_depth = paren_depth.saturating_add(1),
            b')' => {
                paren_depth = paren_depth
                    .checked_sub(1)
                    .ok_or(SpecValidationError::InvalidEventSignature)?;
            }
            b'[' => bracket_depth = bracket_depth.saturating_add(1),
            b']' => {
                bracket_depth = bracket_depth
                    .checked_sub(1)
                    .ok_or(SpecValidationError::InvalidEventSignature)?;
            }
            b',' if paren_depth == 0 && bracket_depth == 0 => {
                let seg = trimmed[seg_start..idx].trim();
                if seg.is_empty() {
                    return Err(SpecValidationError::InvalidEventSignature);
                }
                segments.push(seg);
                seg_start = idx.saturating_add(1);
            }
            _ => {}
        }
    }
    if paren_depth != 0 || bracket_depth != 0 {
        return Err(SpecValidationError::InvalidEventSignature);
    }
    let final_seg = trimmed[seg_start..].trim();
    if final_seg.is_empty() {
        return Err(SpecValidationError::InvalidEventSignature);
    }
    segments.push(final_seg);

    let mut out_params = Vec::with_capacity(segments.len());
    for seg in segments {
        let tuple_rest = if seg.starts_with('(') {
            Some(seg)
        } else if let Some(after_kw) = seg.strip_prefix("tuple") {
            let after_ws = after_kw.trim_start();
            after_ws.starts_with('(').then_some(after_ws)
        } else {
            None
        };

        if let Some(tuple_src) = tuple_rest {
            let close_idx = find_matching_close_paren(tuple_src)?;
            let tuple_inner = &tuple_src[1..close_idx];
            let canon_components =
                canonicalize_abi_param_list(tuple_inner, depth.saturating_add(1))?;
            let after_paren = &tuple_src[close_idx.saturating_add(1)..];
            let (brackets, tail) = consume_array_suffixes(after_paren)?;
            validate_trailing_param_tokens(tail)?;
            out_params.push(format!("({canon_components}){brackets}"));
        } else {
            let name_end = seg
                .find(|c: char| c.is_ascii_whitespace() || c == '[')
                .unwrap_or(seg.len());
            let raw_base = &seg[..name_end];
            let canon_base = normalize_elementary_abi_type(raw_base)
                .ok_or(SpecValidationError::InvalidEventSignature)?;
            let (brackets, tail) = consume_array_suffixes(&seg[name_end..])?;
            validate_trailing_param_tokens(tail)?;
            out_params.push(format!("{canon_base}{brackets}"));
        }
    }

    Ok(out_params.join(","))
}

/// Canonicalize a human-readable Solidity event declaration or signature into its exact
/// `keccak256` ABI signature form (e.g. `"event Sale(address indexed buyer, address indexed referrer, uint amount);"`
/// -> `"Sale(address,address,uint256)"`).
pub fn canonicalize_solidity_event_signature(raw: &str) -> Result<String, SpecValidationError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 512 || !s.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err(SpecValidationError::InvalidEventSignature);
    }
    let s = s.trim_end_matches(';').trim();
    let s = match s.strip_prefix("event") {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_whitespace()) => rest.trim_start(),
        _ => s,
    };
    let s = match s.strip_suffix("anonymous") {
        Some(prefix) if prefix.ends_with(|c: char| c.is_ascii_whitespace() || c == ')') => {
            prefix.trim_end()
        }
        _ => s,
    };

    let open_idx = s
        .find('(')
        .ok_or(SpecValidationError::InvalidEventSignature)?;
    let event_name = s[..open_idx].trim();
    if !is_valid_solidity_ident(event_name) {
        return Err(SpecValidationError::InvalidEventSignature);
    }
    let paren_slice = &s[open_idx..];
    let close_idx = find_matching_close_paren(paren_slice)?;
    if close_idx.saturating_add(1) != paren_slice.len() {
        return Err(SpecValidationError::InvalidEventSignature);
    }
    let inner = &paren_slice[1..close_idx];
    let canon_params = canonicalize_abi_param_list(inner, 0)?;
    let canon = format!("{event_name}({canon_params})");
    if canon.len() > 256 {
        return Err(SpecValidationError::InvalidEventSignature);
    }
    Ok(canon)
}

/// Normalize an EVM 256-bit unsigned predicate word from:
/// - full 66-char `0x` hex (`"0x0000...11c37937e08000"`),
/// - unpadded `0x` hex (`"0x11c37937e08000"`), or
/// - base-10 unsigned decimal integer string (`"5000000000000000"`, up to `2^256 - 1`),
///
/// into canonical lowercase 66-char `0x`-prefixed hex.
pub fn normalize_uint256_value_hex(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return None;
    }
    if let Some(hex_body) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if hex_body.is_empty() || hex_body.len() > 64 {
            return None;
        }
        if !hex_body.bytes().all(|b| crypto::hex_nibble(b).is_some()) {
            return None;
        }
        let pad_zeros = 64usize.saturating_sub(hex_body.len());
        let mut out = String::with_capacity(66);
        out.push_str("0x");
        for _ in 0..pad_zeros {
            out.push('0');
        }
        for b in hex_body.bytes() {
            out.push((b as char).to_ascii_lowercase());
        }
        return Some(out);
    }

    if !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Fast u128 path for values <= 38 digits, falling back to full 256-bit decimal accumulator.
    if trimmed.len() <= 38 {
        if let Ok(val) = trimmed.parse::<u128>() {
            let mut word = [0u8; 32];
            word[16..32].copy_from_slice(&val.to_be_bytes());
            return Some(encode_0x_hex(&word));
        }
    }
    let mut acc = [0u8; 32];
    for &b in trimmed.as_bytes() {
        let digit = u16::from(b.saturating_sub(b'0'));
        let mut carry = digit;
        let mut idx = 32usize;
        while idx > 0 {
            idx = idx.saturating_sub(1);
            let val = u16::from(acc[idx]).saturating_mul(10).saturating_add(carry);
            acc[idx] = (val & 0xff) as u8;
            carry = val >> 8;
        }
        if carry != 0 {
            return None;
        }
    }
    Some(encode_0x_hex(&acc))
}

impl OnChainActionSpec {
    /// Normalize all hex fields (`target_contract`, `topic0`, `value_hex`, discriminators)
    /// and Solidity `event_signature` in-place so deserialized specs and JCS commitments
    /// are case- and representation-invariant.
    pub fn canonicalize(&mut self) {
        match &mut self.predicate {
            TargetPredicate::EvmLog(p) => {
                p.target_contract = p.target_contract.to_ascii_lowercase();
                p.topic0 = p.topic0.to_ascii_lowercase();
                if let Some(sig) = &mut p.event_signature {
                    if let Ok(canon) = canonicalize_solidity_event_signature(sig) {
                        *sig = canon;
                    }
                }
                for fp in &mut p.field_predicates {
                    fp.value_hex = normalize_uint256_value_hex(&fp.value_hex)
                        .unwrap_or_else(|| fp.value_hex.to_ascii_lowercase());
                }
            }
            TargetPredicate::SolanaIx(p) => {
                match &mut p.matcher {
                    SolanaEventMatcher::AnchorEmitCpi {
                        event_discriminator_hex,
                    } => {
                        *event_discriminator_hex = event_discriminator_hex.to_ascii_lowercase();
                    }
                    SolanaEventMatcher::InstructionData {
                        ix_discriminator_hex,
                    } => {
                        *ix_discriminator_hex = ix_discriminator_hex.to_ascii_lowercase();
                    }
                }
                for fp in &mut p.field_predicates {
                    fp.value_hex = fp.value_hex.to_ascii_lowercase();
                }
            }
        }
    }

    /// True if this spec evaluates transaction-wide value (`EvmFieldRef::TxValue`
    /// or `min_lamports > 0`), requiring transaction-level replay locking rather
    /// than per-log / per-instruction replay locking.
    pub fn uses_tx_wide_value(&self) -> bool {
        match &self.predicate {
            TargetPredicate::EvmLog(p) => p
                .field_predicates
                .iter()
                .any(|fp| matches!(fp.field, EvmFieldRef::TxValue)),
            TargetPredicate::SolanaIx(p) => p.min_lamports > 0,
        }
    }

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

        if let Some(ts) = self.min_block_timestamp {
            if !(0..=MAX_SAFE_JS_INTEGER_I64).contains(&ts) {
                return Err(SpecValidationError::InvalidMinBlockTimestamp(ts));
            }
        }

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
    let contract_lower = p.target_contract.to_ascii_lowercase();
    if DENYLISTED_EVM_CONTRACTS.contains(&contract_lower.as_str()) {
        return Err(SpecValidationError::DenylistedTargetContract);
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
        canonicalize_solidity_event_signature(sig)?;
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
        if normalize_uint256_value_hex(&fp.value_hex).is_none() {
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

    if p.min_lamports > MAX_SAFE_JS_INTEGER_U64 {
        return Err(SpecValidationError::MinLamportsExceedsSafeJsInteger(
            p.min_lamports,
        ));
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

    if let Some(recip_idx) = p.recipient_account_index {
        if recip_idx >= 32 {
            return Err(SpecValidationError::InvalidAccountIndex(recip_idx));
        }
        if recip_idx == p.actor_account_index {
            return Err(SpecValidationError::RecipientEqualsActorOrAttribution);
        }
        if matches!(p.agent_attribution, SolanaAttributionSource::AccountIndex { index } if index == recip_idx)
        {
            return Err(SpecValidationError::RecipientEqualsActorOrAttribution);
        }
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
            min_block_timestamp: None,
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
            min_block_timestamp: None,
            predicate: TargetPredicate::SolanaIx(SolanaProgramPredicate {
                program_id: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
                matcher: SolanaEventMatcher::InstructionData {
                    ix_discriminator_hex: "0x04".to_string(),
                },
                agent_attribution: SolanaAttributionSource::AccountIndex { index: 1 },
                actor_account_index: 0,
                min_lamports: 0,
                recipient_account_index: None,
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
            min_block_timestamp: Some(1_765_000_000),
            predicate: TargetPredicate::SolanaIx(SolanaProgramPredicate {
                program_id: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P".to_string(),
                matcher: SolanaEventMatcher::AnchorEmitCpi {
                    event_discriminator_hex: "0x0102030405060708".to_string(),
                },
                agent_attribution: SolanaAttributionSource::PayloadPubkey { byte_offset: 0 },
                actor_account_index: 0,
                min_lamports: 10_000_000,
                recipient_account_index: Some(2),
                field_predicates: vec![SolanaSlicePredicate {
                    byte_offset: 32,
                    encoding: SolanaSliceEncoding::LeU64,
                    op: PredicateOp::Gte,
                    value_hex: "0x0000000000989680".to_string(),
                }],
            }),
        };
        spec.validate().expect("Solana emit_cpi spec valid");
        assert!(spec.uses_tx_wide_value());
        assert_ne!(spec.spec_commitment_sha256(), [0u8; 32]);
    }

    #[test]
    fn rejects_permit2_stake_program_and_unsafe_js_integers() {
        let mut permit2_spec = sample_bsc_affiliate_spec();
        if let TargetPredicate::EvmLog(ref mut p) = permit2_spec.predicate {
            p.target_contract = "0x000000000022D473030F116dDEE9F6B43aC78BA3".to_string();
        }
        assert_eq!(
            permit2_spec.validate(),
            Err(SpecValidationError::DenylistedTargetContract)
        );

        let stake_spec = OnChainActionSpec {
            schema_version: ONCHAIN_ACTION_SCHEMA_VERSION,
            target_chain: "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp".to_string(),
            action_mode: ActionMode::AffiliateEvent,
            min_block_timestamp: None,
            predicate: TargetPredicate::SolanaIx(SolanaProgramPredicate {
                program_id: "Stake11111111111111111111111111111111111111".to_string(),
                matcher: SolanaEventMatcher::InstructionData {
                    ix_discriminator_hex: "0x01".to_string(),
                },
                agent_attribution: SolanaAttributionSource::AccountIndex { index: 1 },
                actor_account_index: 0,
                min_lamports: 0,
                recipient_account_index: None,
                field_predicates: vec![],
            }),
        };
        assert_eq!(
            stake_spec.validate(),
            Err(SpecValidationError::DenylistedProgramId)
        );

        let overflow_js = OnChainActionSpec {
            schema_version: ONCHAIN_ACTION_SCHEMA_VERSION,
            target_chain: "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp".to_string(),
            action_mode: ActionMode::AffiliateEvent,
            min_block_timestamp: None,
            predicate: TargetPredicate::SolanaIx(SolanaProgramPredicate {
                program_id: "2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P".to_string(),
                matcher: SolanaEventMatcher::InstructionData {
                    ix_discriminator_hex: "0x01".to_string(),
                },
                agent_attribution: SolanaAttributionSource::AccountIndex { index: 1 },
                actor_account_index: 0,
                min_lamports: MAX_SAFE_JS_INTEGER_U64.saturating_add(1),
                recipient_account_index: Some(2),
                field_predicates: vec![],
            }),
        };
        assert_eq!(
            overflow_js.validate(),
            Err(SpecValidationError::MinLamportsExceedsSafeJsInteger(
                MAX_SAFE_JS_INTEGER_U64.saturating_add(1)
            ))
        );
    }

    #[test]
    fn proof_ref_parses_all_six_shapes_and_resolves_auto_index_replay_locks() {
        assert_eq!(MAX_RECEIPT_LOGS_SCANNED, 256);
        assert_eq!(MAX_SOLANA_IX_SCANNED, 256);

        let tx_hex = "0xABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let tx_lower = "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let sig_bytes = [0x42u8; 65];
        let sig_b64 = encode_base64url_nopad(&sig_bytes);
        let sig_0x = encode_0x_hex(&sig_bytes);
        let sig_raw_hex = &sig_0x[2..];

        // U2 gate: strict `parse()` rejects omitted / `:auto` until downstream verifiers opt into `parse_allowing_auto()`.
        assert_eq!(
            OnChainProofRef::parse(&format!("eip155:56:{tx_hex}")),
            Err(ProofParseError::InvalidFormat)
        );
        assert_eq!(
            OnChainProofRef::parse(&format!("eip155:56:{tx_hex}:auto")),
            Err(ProofParseError::InvalidIndex)
        );
        assert_eq!(
            OnChainProofRef::parse(&format!("eip155:56:{tx_hex}:sig:{sig_b64}")),
            Err(ProofParseError::InvalidFormat)
        );
        assert_eq!(
            OnChainProofRef::parse(&format!("eip155:56:{tx_hex}:auto:sig:{sig_0x}")),
            Err(ProofParseError::InvalidIndex)
        );

        // 1. <ns>:<ref>:<tx_id> via `parse_allowing_auto` (3 parts -> auto_index = true)
        let p3 =
            OnChainProofRef::parse_allowing_auto(&format!("eip155:56:{tx_hex}")).expect("3-part");
        assert_eq!(p3.tx_id, tx_lower);
        assert_eq!(p3.index, 0);
        assert!(p3.auto_index);
        assert!(p3.attribution_sig.is_none());

        // 2. <ns>:<ref>:<tx_id>:auto (4 parts -> auto_index = true)
        let p4_auto = OnChainProofRef::parse_allowing_auto(&format!("eip155:56:{tx_hex}:auto"))
            .expect("4-auto");
        assert_eq!(p4_auto, p3);

        // 3. <ns>:<ref>:<tx_id>:<u32> (4 parts -> explicit index, up to 255)
        let p4_idx = OnChainProofRef::parse(&format!("eip155:56:{tx_hex}:255")).expect("4-idx 255");
        assert_eq!(p4_idx.index, 255);
        assert!(!p4_idx.auto_index);
        assert!(p4_idx.attribution_sig.is_none());
        assert_eq!(
            OnChainProofRef::parse(&format!("eip155:56:{tx_hex}:256")),
            Err(ProofParseError::IndexOutOfBounds {
                index: 256,
                max: 256
            })
        );

        // 4. <ns>:<ref>:<tx_id>:sig:<sig> (5 parts -> auto_index = true, base64url & 0x hex & raw hex)
        let p5_b64 =
            OnChainProofRef::parse_allowing_auto(&format!("eip155:56:{tx_hex}:sig:{sig_b64}"))
                .expect("5-b64");
        let p5_0x =
            OnChainProofRef::parse_allowing_auto(&format!("eip155:56:{tx_hex}:sig:{sig_0x}"))
                .expect("5-0x");
        let p5_raw =
            OnChainProofRef::parse_allowing_auto(&format!("eip155:56:{tx_hex}:sig:{sig_raw_hex}"))
                .expect("5-raw");
        assert!(p5_b64.auto_index);
        assert_eq!(p5_b64.index, 0);
        assert_eq!(p5_b64.attribution_sig, Some(sig_bytes));
        assert_eq!(p5_0x, p5_b64);
        assert_eq!(p5_raw, p5_b64);

        // 5. <ns>:<ref>:<tx_id>:auto:sig:<sig> (6 parts -> auto_index = true)
        let p6_auto =
            OnChainProofRef::parse_allowing_auto(&format!("eip155:56:{tx_hex}:auto:sig:{sig_0x}"))
                .expect("6-auto-0x");
        assert_eq!(p6_auto, p5_b64);

        // 6. <ns>:<ref>:<tx_id>:<u32>:sig:<sig> (6 parts -> explicit index, accepted by BOTH parse & parse_allowing_auto)
        let p6_idx = OnChainProofRef::parse(&format!("eip155:56:{tx_hex}:199:sig:{sig_0x}"))
            .expect("6-idx-199");
        assert!(!p6_idx.auto_index);
        assert_eq!(p6_idx.index, 199);
        assert_eq!(p6_idx.attribution_sig, Some(sig_bytes));

        // Pre-resolution `auto` lock must NOT collide with explicit index 0, while
        // `with_resolved_index(0)` matches explicit index 0!
        let p4_zero = OnChainProofRef::parse(&format!("eip155:56:{tx_hex}:0")).expect("4-idx-0");
        let target = "0xbe9a615678b2f3f06196b2de3f331c2c8c149d56";
        assert_ne!(p3.replay_lock_id(target), p4_zero.replay_lock_id(target));
        assert_eq!(
            p3.with_resolved_index(0).replay_lock_id(target),
            p4_zero.replay_lock_id(target)
        );

        // Reject malformed 5-part without "sig" and Solana with :sig:
        assert_eq!(
            OnChainProofRef::parse_allowing_auto(&format!("eip155:56:{tx_hex}:not_sig:{sig_b64}")),
            Err(ProofParseError::InvalidFormat)
        );
        let sol_sig = encode_base58_test_64();
        let sol_auto = OnChainProofRef::parse_allowing_auto(&format!(
            "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp:{sol_sig}"
        ))
        .expect("solana 3-part");
        assert!(sol_auto.auto_index);
        assert_eq!(
            OnChainProofRef::parse_allowing_auto(&format!(
                "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp:{sol_sig}:sig:{sig_b64}"
            )),
            Err(ProofParseError::AttributionSigNotAllowedOnSolana)
        );
    }

    fn encode_base58_test_64() -> String {
        // 64 bytes of 0x01 encoded in Bitcoin/Solana base58 (`decode_base58_exact::<64>` valid).
        const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        let mut num = [1u8; 64];
        let mut digits = Vec::new();
        while num.iter().any(|&b| b != 0) {
            let mut rem = 0u32;
            for byte in &mut num {
                let acc = (rem << 8) | u32::from(*byte);
                *byte = (acc / 58) as u8;
                rem = acc % 58;
            }
            digits.push(ALPHABET[rem as usize]);
        }
        digits.reverse();
        String::from_utf8(digits).expect("ascii")
    }

    #[cfg(feature = "cosign")]
    #[test]
    fn proof_ref_parses_and_recovers_stateless_eip191_attribution_sig_for_b64_and_hex() {
        let mut sk = [0u8; 32];
        sk[31] = 0x55;
        let expected_evm = crate::cosign::eth_address(&sk).unwrap();
        let chain = ChainId::parse("eip155:56").unwrap();
        let agent_b58 = "HCqPmPJM3NVtS6YrmVaTZYnuP8XZ2PFgQZKT94VK1LkS";

        let sig_b64 = sign_evm_attribution(&sk, &chain, agent_b58).unwrap();
        let sig_bytes = decode_base64url_sig65(&sig_b64).unwrap();
        let sig_0x = encode_0x_hex(&sig_bytes);

        let content_id_b64 = format!(
            "eip155:56:0xABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789:2:sig:{sig_b64}"
        );
        let content_id_hex_omitted_idx = format!(
            "eip155:56:0xABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789:sig:{sig_0x}"
        );
        assert!(content_id_b64.len() <= MAX_PROOF_CONTENT_ID_BYTES);
        assert!(content_id_hex_omitted_idx.len() <= MAX_PROOF_CONTENT_ID_BYTES);

        let parsed_b64 = OnChainProofRef::parse(&content_id_b64).expect("valid b64 proof ref");
        let parsed_hex = OnChainProofRef::parse_allowing_auto(&content_id_hex_omitted_idx)
            .expect("valid 0x hex proof ref");
        assert!(!parsed_b64.auto_index);
        assert_eq!(parsed_b64.index, 2);
        assert!(parsed_hex.auto_index);

        let recovered_b64 = parsed_b64
            .recover_evm_attribution(agent_b58)
            .expect("recovery ok")
            .expect("sig present");
        let recovered_hex = parsed_hex
            .recover_evm_attribution(agent_b58)
            .expect("recovery ok")
            .expect("sig present");
        assert_eq!(recovered_b64, expected_evm);
        assert_eq!(recovered_hex, expected_evm);

        // Submitting the same :sig: under a DIFFERENT Solana agent pubkey recovers a
        // random address != expected_evm (defeating mempool frontrunning!).
        let frontrunner_b58 = "5vsGoTRoc5j1a2fKszyZ7y28G6ggmu87YobpwzuXsMhu";
        let wrong = parsed_hex
            .recover_evm_attribution(frontrunner_b58)
            .expect("recovery ok")
            .expect("sig present");
        assert_ne!(wrong, expected_evm);
    }

    #[test]
    fn canonicalizes_solidity_event_signatures_and_rejects_malformed() {
        assert_eq!(
            canonicalize_solidity_event_signature(
                "event Sale(address indexed buyer, address indexed referrer, uint amount);"
            )
            .unwrap(),
            "Sale(address,address,uint256)"
        );
        assert_eq!(
            canonicalize_solidity_event_signature(
                "  Transfer ( address indexed from , address indexed to , int value ) anonymous ; "
            )
            .unwrap(),
            "Transfer(address,address,int256)"
        );
        assert_eq!(
            canonicalize_solidity_event_signature(
                "Batch((address user, uint amt)[] indexed items, uint[4] counts)"
            )
            .unwrap(),
            "Batch((address,uint256)[],uint256[4])"
        );
        assert_eq!(
            canonicalize_solidity_event_signature(
                "event Nested(tuple(address who, (uint a, int[] b) inner) payload)"
            )
            .unwrap(),
            "Nested((address,(uint256,int256[])))"
        );
        assert_eq!(
            canonicalize_solidity_event_signature("Paused()").unwrap(),
            "Paused()"
        );

        for malformed in [
            "",
            "event ()",
            "(address,uint256)",
            "Sale(address",
            "Sale(address))",
            "Sale(address,)",
            "Sale(,address)",
            "Sale(indexed buyer)",
            "Sale(())",
            "Sale(uint[abc] amount)",
            "Sale(uint a b)",
            "123Bad(address)",
        ] {
            assert_eq!(
                canonicalize_solidity_event_signature(malformed),
                Err(SpecValidationError::InvalidEventSignature),
                "must reject {malformed:?}"
            );
        }
    }

    #[test]
    fn normalizes_uint256_predicate_values_and_preserves_jcs_commitment_parity() {
        let expected_bnb = "0x0000000000000000000000000000000000000000000000000011c37937e08000";
        assert_eq!(
            normalize_uint256_value_hex("5000000000000000").as_deref(),
            Some(expected_bnb)
        );
        assert_eq!(
            normalize_uint256_value_hex("0x11C37937E08000").as_deref(),
            Some(expected_bnb)
        );
        assert_eq!(
            normalize_uint256_value_hex(expected_bnb).as_deref(),
            Some(expected_bnb)
        );
        assert_eq!(
            normalize_uint256_value_hex("0").as_deref(),
            Some("0x0000000000000000000000000000000000000000000000000000000000000000")
        );
        assert_eq!(
            normalize_uint256_value_hex("0x0").as_deref(),
            Some("0x0000000000000000000000000000000000000000000000000000000000000000")
        );

        // Exact u256::MAX (2^256 - 1) succeeds; 2^256 overflows and returns None!
        let u256_max_dec =
            "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        let u256_overflow_dec =
            "115792089237316195423570985008687907853269984665640564039457584007913129639936";
        assert_eq!(
            normalize_uint256_value_hex(u256_max_dec).as_deref(),
            Some("0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
        );
        assert!(normalize_uint256_value_hex(u256_overflow_dec).is_none());
        assert!(normalize_uint256_value_hex(&format!("0x1{}", "0".repeat(64))).is_none());
        for bad in ["", "0x", "-1", "5_000", "deadbeef", "0x123g"] {
            assert!(
                normalize_uint256_value_hex(bad).is_none(),
                "must reject {bad:?}"
            );
        }

        // Specs written with decimal wei, unpadded 0x hex, and human-readable event_signature
        // validate AND produce the exact same JCS commitment as the canonical 66-char spec!
        let canonical_spec = sample_bsc_affiliate_spec();
        let mut ergonomic_spec = sample_bsc_affiliate_spec();
        if let TargetPredicate::EvmLog(ref mut p) = ergonomic_spec.predicate {
            p.event_signature = Some(
                "event AffiliateMint(address indexed buyer, address indexed referrer, uint amount);"
                    .to_string(),
            );
            p.field_predicates[0].value_hex = "5000000000000000".to_string();
        }
        ergonomic_spec
            .validate()
            .expect("decimal wei + Solidity decl valid");
        assert_eq!(
            ergonomic_spec.spec_commitment_sha256(),
            canonical_spec.spec_commitment_sha256()
        );
        ergonomic_spec.canonicalize();
        assert_eq!(ergonomic_spec, canonical_spec.with_canonicalized());
    }

    impl OnChainActionSpec {
        fn with_canonicalized(&self) -> Self {
            let mut c = self.clone();
            c.canonicalize();
            c
        }
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
