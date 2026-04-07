//! Local Ed25519 signer for an unsigned base64 Solana transaction.
//!
//! Use during a manual MCP walkthrough: claim_task / submit_work return
//! base64-encoded unsigned txs that the agent must sign locally before
//! handing back to shillbot_submit_tx. The Solana CLI has no
//! "sign-this-serialized-tx" command, so this example fills the gap with the
//! solana-sdk types already in the workspace.
//!
//! Usage:
//!     cargo run --release -p mcp-server --example sign_tx -- <base64-tx>
//!
//! Reads the keypair from `~/.config/solana/id.json` (the standard Solana
//! CLI default). Prints the signed tx as base64 on stdout — pipe straight
//! into `mcp__swarm-tips__shillbot_submit_tx` as the `signed_transaction`
//! argument.

use base64::Engine;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(unsigned_b64) = args.next() else {
        eprintln!("usage: sign_tx <base64-unsigned-tx>");
        return ExitCode::from(2);
    };

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
