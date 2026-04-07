//! Local Ed25519 signer for an unsigned base64 Solana transaction.
//!
//! Use during a manual MCP walkthrough: claim_task / submit_work return
//! base64-encoded unsigned txs that the agent must sign locally before
//! handing back to shillbot_submit_tx. The Solana CLI has no
//! "sign-this-serialized-tx" command, so this example fills the gap with the
//! solana-sdk types already in the workspace.
//!
//! Usage:
//!     cargo run --release -p mcp-server --example sign_tx -- <base64-tx> [<cosign-pubkey>:<cosign-sig-b64>]
//!
//! Reads the keypair from `~/.config/solana/id.json` (the standard Solana
//! CLI default). Prints the signed tx as base64 on stdout — pipe straight
//! into `mcp__swarm-tips__shillbot_submit_tx` / `game_submit_tx` as the
//! `signed_transaction` argument.
//!
//! The optional second arg injects a pre-computed cosignature into the slot
//! matching the given pubkey. Used for multi-signer flows like
//! `create_game`, where the game-api pre-signs as the matchmaker and the
//! player must add their own signature without recomputing the message.

use base64::Engine;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature, Signer};
use solana_sdk::transaction::Transaction;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(unsigned_b64) = args.next() else {
        eprintln!("usage: sign_tx <base64-unsigned-tx> [<cosign-pubkey>:<cosign-sig-b64>]");
        return ExitCode::from(2);
    };
    let cosign_spec = args.next();

    let keypair_path: PathBuf = env::var("SOLANA_KEYPAIR_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = env::var("HOME").expect("HOME must be set");
            PathBuf::from(home).join(".config/solana/id.json")
        });

    let keypair_bytes_json = match fs::read_to_string(&keypair_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read keypair {}: {e}", keypair_path.display());
            return ExitCode::from(1);
        }
    };
    let keypair_bytes: Vec<u8> = match serde_json::from_str(&keypair_bytes_json) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("parse keypair JSON: {e}");
            return ExitCode::from(1);
        }
    };
    let keypair = match Keypair::try_from(keypair_bytes.as_slice()) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("invalid keypair bytes: {e}");
            return ExitCode::from(1);
        }
    };

    let tx_bytes = match base64::engine::general_purpose::STANDARD.decode(unsigned_b64.trim()) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("decode base64 tx: {e}");
            return ExitCode::from(1);
        }
    };
    let mut tx: Transaction = match bincode::deserialize(&tx_bytes) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("deserialize tx: {e}");
            return ExitCode::from(1);
        }
    };

    // Inject any pre-computed cosignature (e.g. matchmaker for create_game)
    // before we partial_sign ourselves. We write directly into the signatures
    // vector at the slot matching the cosigner's position in account_keys so
    // we don't recompute the message (which would invalidate the cosign).
    if let Some(spec) = cosign_spec {
        let Some((pubkey_str, sig_b64)) = spec.split_once(':') else {
            eprintln!("cosign must be \"<pubkey>:<base64-sig>\"");
            return ExitCode::from(2);
        };
        let pubkey = match Pubkey::from_str(pubkey_str) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("invalid cosign pubkey: {e}");
                return ExitCode::from(1);
            }
        };
        let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(sig_b64) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("decode cosign sig: {e}");
                return ExitCode::from(1);
            }
        };
        let Ok(sig_array) = <[u8; 64]>::try_from(sig_bytes.as_slice()) else {
            eprintln!("cosign signature must be 64 bytes, got {}", sig_bytes.len());
            return ExitCode::from(1);
        };
        let signature = Signature::from(sig_array);
        let Some(idx) = tx.message.account_keys.iter().position(|k| *k == pubkey) else {
            eprintln!("cosign pubkey {pubkey} not found in tx account keys");
            return ExitCode::from(1);
        };
        if idx >= tx.signatures.len() {
            eprintln!(
                "cosign pubkey {pubkey} at index {idx} is not a signer (only {} sig slots)",
                tx.signatures.len()
            );
            return ExitCode::from(1);
        }
        tx.signatures[idx] = signature;
    }

    // Sign in place. partial_sign respects the existing signers list and
    // injects our signature into the slot whose pubkey matches the keypair.
    let blockhash = tx.message.recent_blockhash;
    tx.partial_sign(&[&keypair], blockhash);

    if !tx.is_signed() {
        eprintln!(
            "tx not fully signed after partial_sign — wallet {} may not match any required signer",
            keypair.pubkey()
        );
        return ExitCode::from(1);
    }

    let signed_bytes = match bincode::serialize(&tx) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("serialize signed tx: {e}");
            return ExitCode::from(1);
        }
    };
    let signed_b64 = base64::engine::general_purpose::STANDARD.encode(&signed_bytes);
    println!("{signed_b64}");
    ExitCode::SUCCESS
}
