//! Deterministic RFC 8785 JSON Canonicalization Scheme (JCS) serializer for
//! [`OnChainActionSpec`].
//!
//! Because [`OnChainActionSpec`] contains only strings, booleans, unsigned
//! integers (`u8`, `u16`, `u64`), arrays, and fixed-key structs/enums (zero
//! floating-point numbers), RFC 8785 reduces to:
//! 1. Zero whitespace between tokens;
//! 2. Minimal RFC 8259 string escaping (`\b`, `\t`, `\n`, `\f`, `\r`, `\"`, `\\`, `\u00xx`);
//! 3. Object keys sorted strictly by UTF-16 code-unit order (identical to ASCII
//!    byte order for all ASCII keys).

use crate::onchain_action::{
    ActionMode, EvmAttributionSource, EvmFieldPredicate, EvmFieldRef, EvmLogPredicate,
    OnChainActionSpec, PredicateOp, SolanaAttributionSource, SolanaEventMatcher,
    SolanaProgramPredicate, SolanaSliceEncoding, SolanaSlicePredicate, TargetPredicate,
};

/// Escape and append a JSON string per RFC 8785 §3.2.2.2.
pub fn push_jcs_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{0009}' => out.push_str("\\t"),
            '\u{000a}' => out.push_str("\\n"),
            '\u{000c}' => out.push_str("\\f"),
            '\u{000d}' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => {
                let code = c as u32;
                const HEX: &[u8; 16] = b"0123456789abcdef";
                out.push_str("\\u00");
                out.push(HEX[((code >> 4) & 0x0f) as usize] as char);
                out.push(HEX[(code & 0x0f) as usize] as char);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_op(out: &mut String, op: PredicateOp) {
    let s = match op {
        PredicateOp::Eq => "eq",
        PredicateOp::Ne => "ne",
        PredicateOp::Gte => "gte",
    };
    push_jcs_str(out, s);
}

fn push_evm_field_ref(out: &mut String, f: &EvmFieldRef) {
    match f {
        EvmFieldRef::TxFrom => out.push_str("{\"field\":\"tx_from\"}"),
        EvmFieldRef::TxValue => out.push_str("{\"field\":\"tx_value\"}"),
        EvmFieldRef::Topic { index } => {
            // Keys: "field" < "index"
            out.push_str("{\"field\":\"topic\",\"index\":");
            out.push_str(&index.to_string());
            out.push('}');
        }
        EvmFieldRef::DataWord { word_index } => {
            // Keys: "field" < "word_index"
            out.push_str("{\"field\":\"data_word\",\"word_index\":");
            out.push_str(&word_index.to_string());
            out.push('}');
        }
    }
}

fn push_evm_attribution(out: &mut String, attr: &EvmAttributionSource) {
    match attr {
        EvmAttributionSource::Topic { index } => {
            // Keys: "index" < "source"
            out.push_str("{\"index\":");
            out.push_str(&index.to_string());
            out.push_str(",\"source\":\"topic\"}");
        }
        EvmAttributionSource::DataWord { word_index } => {
            // Keys: "source" < "word_index"
            out.push_str("{\"source\":\"data_word\",\"word_index\":");
            out.push_str(&word_index.to_string());
            out.push('}');
        }
    }
}

fn push_evm_predicate_item(out: &mut String, item: &EvmFieldPredicate) {
    // Keys: "field" < "op" < "value_hex"
    out.push_str("{\"field\":");
    push_evm_field_ref(out, &item.field);
    out.push_str(",\"op\":");
    push_op(out, item.op);
    out.push_str(",\"value_hex\":");
    push_jcs_str(out, &item.value_hex.to_ascii_lowercase());
    out.push('}');
}

fn push_evm_log_predicate(out: &mut String, p: &EvmLogPredicate) {
    // Keys in strict ASCII/UTF-16 order:
    // "actor_field" < "agent_attribution" < ("event_signature") < "field_predicates"
    // < "kind" < "require_direct_call" < "target_contract" < "topic0"
    out.push_str("{\"actor_field\":");
    push_evm_field_ref(out, &p.actor_field);
    out.push_str(",\"agent_attribution\":");
    push_evm_attribution(out, &p.agent_attribution);
    if let Some(sig) = &p.event_signature {
        out.push_str(",\"event_signature\":");
        push_jcs_str(out, sig);
    }
    out.push_str(",\"field_predicates\":[");
    for (idx, fp) in p.field_predicates.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        push_evm_predicate_item(out, fp);
    }
    out.push_str("],\"kind\":\"evm_log\",\"require_direct_call\":");
    out.push_str(if p.require_direct_call {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"target_contract\":");
    push_jcs_str(out, &p.target_contract.to_ascii_lowercase());
    out.push_str(",\"topic0\":");
    push_jcs_str(out, &p.topic0.to_ascii_lowercase());
    out.push('}');
}

fn push_solana_attribution(out: &mut String, attr: &SolanaAttributionSource) {
    match attr {
        SolanaAttributionSource::AccountIndex { index } => {
            // Keys: "index" < "source"
            out.push_str("{\"index\":");
            out.push_str(&index.to_string());
            out.push_str(",\"source\":\"account_index\"}");
        }
        SolanaAttributionSource::PayloadPubkey { byte_offset } => {
            // Keys: "byte_offset" < "source"
            out.push_str("{\"byte_offset\":");
            out.push_str(&byte_offset.to_string());
            out.push_str(",\"source\":\"payload_pubkey\"}");
        }
    }
}

fn push_solana_matcher(out: &mut String, m: &SolanaEventMatcher) {
    match m {
        SolanaEventMatcher::AnchorEmitCpi {
            event_discriminator_hex,
        } => {
            // Keys: "event_discriminator_hex" < "mode"
            out.push_str("{\"event_discriminator_hex\":");
            push_jcs_str(out, &event_discriminator_hex.to_ascii_lowercase());
            out.push_str(",\"mode\":\"anchor_emit_cpi\"}");
        }
        SolanaEventMatcher::InstructionData {
            ix_discriminator_hex,
        } => {
            // Keys: "ix_discriminator_hex" < "mode"
            out.push_str("{\"ix_discriminator_hex\":");
            push_jcs_str(out, &ix_discriminator_hex.to_ascii_lowercase());
            out.push_str(",\"mode\":\"instruction_data\"}");
        }
    }
}

fn push_solana_slice_item(out: &mut String, item: &SolanaSlicePredicate) {
    // Keys: "byte_offset" < "encoding" < "op" < "value_hex"
    out.push_str("{\"byte_offset\":");
    out.push_str(&item.byte_offset.to_string());
    out.push_str(",\"encoding\":");
    let enc = match item.encoding {
        SolanaSliceEncoding::Be256 => "be256",
        SolanaSliceEncoding::LeU64 => "le_u64",
    };
    push_jcs_str(out, enc);
    out.push_str(",\"op\":");
    push_op(out, item.op);
    out.push_str(",\"value_hex\":");
    push_jcs_str(out, &item.value_hex.to_ascii_lowercase());
    out.push('}');
}

fn push_solana_program_predicate(out: &mut String, p: &SolanaProgramPredicate) {
    // Keys in strict ASCII/UTF-16 order:
    // "actor_account_index" < "agent_attribution" < "field_predicates"
    // < "kind" < "matcher" < "min_lamports" < "program_id" < ("recipient_account_index")
    out.push_str("{\"actor_account_index\":");
    out.push_str(&p.actor_account_index.to_string());
    out.push_str(",\"agent_attribution\":");
    push_solana_attribution(out, &p.agent_attribution);
    out.push_str(",\"field_predicates\":[");
    for (idx, fp) in p.field_predicates.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        push_solana_slice_item(out, fp);
    }
    out.push_str("],\"kind\":\"solana_ix\",\"matcher\":");
    push_solana_matcher(out, &p.matcher);
    out.push_str(",\"min_lamports\":");
    out.push_str(&p.min_lamports.to_string());
    out.push_str(",\"program_id\":");
    push_jcs_str(out, &p.program_id);
    if let Some(recip) = p.recipient_account_index {
        out.push_str(",\"recipient_account_index\":");
        out.push_str(&recip.to_string());
    }
    out.push('}');
}

/// Serialize [`OnChainActionSpec`] into its unique RFC 8785 JCS UTF-8 string.
pub fn serialize_spec_jcs(spec: &OnChainActionSpec) -> String {
    let mut out = String::with_capacity(512);
    // Top-level keys in strict ASCII/UTF-16 order:
    // "action_mode" < ("min_block_timestamp") < "predicate" < "schema_version" < "target_chain"
    out.push_str("{\"action_mode\":");
    match spec.action_mode {
        ActionMode::AffiliateEvent => push_jcs_str(&mut out, "affiliate_event"),
    }
    if let Some(ts) = spec.min_block_timestamp {
        out.push_str(",\"min_block_timestamp\":");
        out.push_str(&ts.to_string());
    }
    out.push_str(",\"predicate\":");
    match &spec.predicate {
        TargetPredicate::EvmLog(p) => push_evm_log_predicate(&mut out, p),
        TargetPredicate::SolanaIx(p) => push_solana_program_predicate(&mut out, p),
    }
    out.push_str(",\"schema_version\":");
    out.push_str(&spec.schema_version.to_string());
    out.push_str(",\"target_chain\":");
    push_jcs_str(&mut out, &spec.target_chain);
    out.push('}');
    debug_assert!(out.starts_with('{') && out.ends_with('}'));
    out
}
