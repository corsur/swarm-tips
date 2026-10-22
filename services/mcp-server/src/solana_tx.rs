use crate::errors::McpServiceError;

/// Broadcast an already-signed transaction (base64-encoded) to mainnet via
/// the configured RPC endpoint and return the resulting signature.
///
/// Used by the non-custodial Shillbot path: the agent receives an unsigned tx
/// from `claim_task` / `submit_work`, signs it locally, then submits the
/// signed bytes here. The MCP server never sees the agent's private key —
/// only the already-signed transaction.
pub async fn broadcast_signed_b64(
    client: &reqwest::Client,
    rpc_url: &str,
    signed_tx_b64: &str,
) -> Result<String, McpServiceError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(signed_tx_b64)
        .map_err(|e| {
            McpServiceError::TransactionError(format!("invalid base64 signed transaction: {e}"))
        })?;

    // Send the base64 encoding directly — Solana's `sendTransaction` accepts
    // it natively when the encoding param is set, avoiding a base58 round-trip.
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [
            signed_tx_b64,
            { "encoding": "base64", "skipPreflight": false, "preflightCommitment": "confirmed" }
        ],
    });

    let response = client
        .post(rpc_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("send transaction failed: {e}")))?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("send response parse failed: {e}")))?;

    if let Some(error) = json.get("error") {
        return Err(McpServiceError::SolanaRpcError(format!(
            "transaction rejected: {error}"
        )));
    }

    json["result"]
        .as_str()
        .ok_or_else(|| McpServiceError::SolanaRpcError("missing signature in response".to_string()))
        .map(|s| s.to_string())
}

/// Poll `getSignatureStatuses` until the given signature reports `confirmed`
/// (or higher) commitment. Returns `Ok(())` once visible, or an error after
/// `max_attempts * 1s` of polling. Used between broadcast and orchestrator
/// confirm to avoid the race where the orchestrator's `verify_tx_confirmed`
/// runs before the tx has propagated to its RPC view.
pub async fn wait_for_signature_confirmed(
    client: &reqwest::Client,
    rpc_url: &str,
    signature: &str,
    max_attempts: u32,
) -> Result<(), McpServiceError> {
    if signature.is_empty() {
        return Err(McpServiceError::TransactionError(
            "signature must not be empty".to_string(),
        ));
    }
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getSignatureStatuses",
        "params": [[signature], { "searchTransactionHistory": true }],
    });

    for attempt in 0..max_attempts {
        let response = client
            .post(rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| McpServiceError::SolanaRpcError(format!("status request failed: {e}")))?;
        let json: serde_json::Value = response.json().await.map_err(|e| {
            McpServiceError::SolanaRpcError(format!("status response parse failed: {e}"))
        })?;

        let entry = json["result"]["value"]
            .as_array()
            .and_then(|arr| arr.first())
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        if !entry.is_null() {
            if let Some(err) = entry.get("err").filter(|v| !v.is_null()) {
                return Err(McpServiceError::SolanaRpcError(format!(
                    "transaction failed on-chain: {err}"
                )));
            }
            let confirmation_status = entry["confirmationStatus"].as_str().unwrap_or("");
            if matches!(confirmation_status, "confirmed" | "finalized") {
                tracing::info!(
                    signature = %signature,
                    attempt = attempt,
                    status = %confirmation_status,
                    "tx confirmed on-chain"
                );
                return Ok(());
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    Err(McpServiceError::SolanaRpcError(format!(
        "transaction {signature} did not reach confirmed commitment within {max_attempts}s"
    )))
}

const COMPUTE_BUDGET_PROGRAM_ID: &str = "ComputeBudget111111111111111111111111111111";

/// Parse a 64-byte Solana keypair JSON array (standard Solana CLI / Secret
/// Manager format) into a `solana_sdk::signature::Keypair`.
pub fn parse_keypair_json(raw: &str) -> Result<solana_sdk::signature::Keypair, McpServiceError> {
    let bytes: Vec<u8> = serde_json::from_str(raw.trim()).map_err(|e| {
        McpServiceError::TransactionError(format!("invalid keypair JSON array: {e}"))
    })?;
    solana_sdk::signature::Keypair::try_from(bytes.as_slice()).map_err(|e| {
        McpServiceError::TransactionError(format!("invalid Solana keypair bytes: {e}"))
    })
}

/// Fetch the latest confirmed blockhash from a Solana RPC endpoint.
pub async fn fetch_latest_blockhash(
    client: &reqwest::Client,
    rpc_url: &str,
) -> Result<solana_sdk::hash::Hash, McpServiceError> {
    use std::str::FromStr;
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestBlockhash",
        "params": [{ "commitment": "confirmed" }],
    });
    let response = client
        .post(rpc_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("getLatestBlockhash failed: {e}")))?;
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| McpServiceError::SolanaRpcError(format!("blockhash parse failed: {e}")))?;
    if let Some(error) = json.get("error") {
        return Err(McpServiceError::SolanaRpcError(format!(
            "getLatestBlockhash error: {error}"
        )));
    }
    let bh_str = json["result"]["value"]["blockhash"]
        .as_str()
        .ok_or_else(|| {
            McpServiceError::SolanaRpcError("missing blockhash in RPC response".to_string())
        })?;
    solana_sdk::hash::Hash::from_str(bh_str)
        .map_err(|e| McpServiceError::SolanaRpcError(format!("invalid blockhash string: {e}")))
}

/// Pure, deterministic builder for a v0 `verify_task` transaction co-signed by
/// `oracle_keypair` with `payer` as the fee payer ($0 protocol gas).
pub fn build_co_signed_verify_tx_with_blockhash(
    payer: &str,
    task_pda: &str,
    composite_score: u64,
    verification_hash_hex: &str,
    oracle_keypair: &solana_sdk::signature::Keypair,
    recent_blockhash: solana_sdk::hash::Hash,
) -> Result<String, McpServiceError> {
    use base64::Engine;
    use solana_sdk::{
        instruction::Instruction,
        message::{v0, VersionedMessage},
        pubkey::Pubkey,
        signature::{Signature, Signer},
        transaction::VersionedTransaction,
    };
    use std::str::FromStr;

    let payer_pubkey = Pubkey::from_str(payer).map_err(|e| {
        McpServiceError::InvalidInput(format!("payer is not a valid Solana pubkey: {e}"))
    })?;
    let task_pubkey = Pubkey::from_str(task_pda).map_err(|e| {
        McpServiceError::InvalidInput(format!("task_pda is not a valid Solana pubkey: {e}"))
    })?;
    let hash_vec = hex::decode(verification_hash_hex.trim()).map_err(|e| {
        McpServiceError::InvalidInput(format!("verification_hash is not valid hex: {e}"))
    })?;
    let verification_hash: [u8; 32] = hash_vec.try_into().map_err(|_| {
        McpServiceError::InvalidInput("verification_hash must be 32 bytes (64 hex chars)".into())
    })?;

    let compute_program = Pubkey::from_str(COMPUTE_BUDGET_PROGRAM_ID).map_err(|e| {
        McpServiceError::TransactionError(format!("invalid compute budget program id: {e}"))
    })?;
    let cu_limit_ix = Instruction {
        program_id: compute_program,
        accounts: vec![],
        data: [vec![2u8], 100_000u32.to_le_bytes().to_vec()].concat(),
    };
    let cu_price_ix = Instruction {
        program_id: compute_program,
        accounts: vec![],
        data: [vec![3u8], 10_000u64.to_le_bytes().to_vec()].concat(),
    };
    let oracle_pubkey = oracle_keypair.pubkey();
    let verify_ix = shillbot_chain::verify_task_co_signed_instruction(
        task_pubkey,
        oracle_pubkey,
        composite_score,
        verification_hash,
    );

    let v0_msg = v0::Message::try_compile(
        &payer_pubkey,
        &[cu_limit_ix, cu_price_ix, verify_ix],
        &[],
        recent_blockhash,
    )
    .map_err(|e| {
        McpServiceError::TransactionError(format!("failed to compile v0 verify message: {e}"))
    })?;
    let message = VersionedMessage::V0(v0_msg);
    let num_required = usize::from(message.header().num_required_signatures);
    let keys = message.static_account_keys();
    let oracle_slot = keys
        .get(..num_required)
        .and_then(|signers| signers.iter().position(|k| *k == oracle_pubkey))
        .ok_or_else(|| {
            McpServiceError::TransactionError(
                "oracle_authority missing from compiled required signers".into(),
            )
        })?;
    let mut signatures = vec![Signature::default(); num_required];
    let msg_bytes = message.serialize();
    if let Some(slot) = signatures.get_mut(oracle_slot) {
        *slot = oracle_keypair.sign_message(&msg_bytes);
    }

    let vtx = VersionedTransaction {
        signatures,
        message,
    };
    let bytes = bincode::serialize(&vtx).map_err(|e| {
        McpServiceError::TransactionError(format!("failed to serialize verify transaction: {e}"))
    })?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Fetch a fresh blockhash and build a co-signed `verify_task` transaction.
pub async fn build_co_signed_verify_tx(
    client: &reqwest::Client,
    rpc_url: &str,
    payer: &str,
    task_pda: &str,
    composite_score: u64,
    verification_hash_hex: &str,
    oracle_keypair: &solana_sdk::signature::Keypair,
) -> Result<String, McpServiceError> {
    let blockhash = fetch_latest_blockhash(client, rpc_url).await?;
    build_co_signed_verify_tx_with_blockhash(
        payer,
        task_pda,
        composite_score,
        verification_hash_hex,
        oracle_keypair,
        blockhash,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcast_rejects_invalid_base64() {
        // Use a tokio runtime so the async fn can be polled in this unit test.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let client = reqwest::Client::new();
        let result = rt.block_on(broadcast_signed_b64(
            &client,
            "https://example.invalid",
            "not!valid!base64!",
        ));
        assert!(matches!(result, Err(McpServiceError::TransactionError(_))));
    }

    // -----------------------------------------------------------------------
    // Flow tests over a mock RPC. wait_for_signature_confirmed is a
    // multi-attempt orchestrator with THREE distinct outcomes (confirmed,
    // failed-on-chain, timeout) and had no coverage at all; broadcast had only
    // the invalid-base64 error path. wiremock is already a dev-dependency.
    //
    // The loop sleeps a real 1s per non-terminal attempt, so the timeout case
    // uses max_attempts=1. The confirmed and on-chain-error cases return before
    // the sleep.
    // -----------------------------------------------------------------------

    use wiremock::matchers::{body_partial_json, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// A valid base64 payload — content is irrelevant, only the decode matters.
    const SIGNED_B64: &str = "AQIDBA==";

    #[tokio::test]
    async fn broadcast_returns_the_signature_from_a_successful_rpc_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_partial_json(
                serde_json::json!({ "method": "sendTransaction" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "result": "sig-123"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let sig = broadcast_signed_b64(&reqwest::Client::new(), &server.uri(), SIGNED_B64)
            .await
            .unwrap();
        assert_eq!(sig, "sig-123");
    }

    #[tokio::test]
    async fn broadcast_surfaces_an_rpc_error_payload_instead_of_a_missing_signature() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "error": { "code": -32002, "message": "blockhash not found" }
            })))
            .mount(&server)
            .await;

        let err = broadcast_signed_b64(&reqwest::Client::new(), &server.uri(), SIGNED_B64)
            .await
            .expect_err("an error payload must not be read as success");
        let msg = format!("{err}");
        assert!(msg.contains("transaction rejected"), "got: {msg}");
        assert!(msg.contains("blockhash not found"), "got: {msg}");
    }

    #[tokio::test]
    async fn wait_returns_ok_once_the_signature_reports_confirmed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "value": [{ "confirmationStatus": "confirmed", "err": null }] }
            })))
            .mount(&server)
            .await;

        wait_for_signature_confirmed(&reqwest::Client::new(), &server.uri(), "sig-1", 3)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn wait_fails_fast_when_the_tx_landed_but_reverted_on_chain() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "value": [{
                    "confirmationStatus": "confirmed",
                    "err": { "InstructionError": [0, { "Custom": 6001 }] }
                }] }
            })))
            .mount(&server)
            .await;

        // A landed-but-failed tx must NOT be reported as confirmed — the caller
        // would then tell the orchestrator the work succeeded.
        let err = wait_for_signature_confirmed(&reqwest::Client::new(), &server.uri(), "sig-1", 3)
            .await
            .expect_err("an on-chain failure must be an error");
        assert!(format!("{err}").contains("failed on-chain"), "got: {err}");
    }

    #[tokio::test]
    async fn wait_times_out_when_the_signature_never_becomes_visible() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "result": { "value": [null] }
            })))
            .mount(&server)
            .await;

        let err = wait_for_signature_confirmed(&reqwest::Client::new(), &server.uri(), "sig-1", 1)
            .await
            .expect_err("an invisible signature must time out");
        assert!(
            format!("{err}").contains("did not reach confirmed"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn wait_rejects_an_empty_signature_without_calling_the_rpc() {
        let server = MockServer::start().await;
        // No mock mounted: any request would 404 and fail differently.
        let err = wait_for_signature_confirmed(&reqwest::Client::new(), &server.uri(), "", 1)
            .await
            .expect_err("empty signature must be rejected");
        assert!(format!("{err}").contains("must not be empty"), "got: {err}");
    }

    #[test]
    fn build_co_signed_verify_tx_partially_signs_oracle_and_leaves_payer_slot_open() {
        use base64::Engine;
        use solana_sdk::hash::Hash;
        use solana_sdk::pubkey::Pubkey;
        use solana_sdk::signature::{Keypair, Signature, Signer};

        let payer = Keypair::new();
        let oracle = Keypair::new();
        let task = Pubkey::new_unique();
        let hash_hex = "ab".repeat(32);

        let b64 = build_co_signed_verify_tx_with_blockhash(
            &payer.pubkey().to_string(),
            &task.to_string(),
            950_000,
            &hash_hex,
            &oracle,
            Hash::new_unique(),
        )
        .expect("co-signed verify tx builds");

        let mut vtx = shillbot_chain::decode_base64(&b64).expect("decodes as VersionedTransaction");
        assert_eq!(vtx.signatures.len(), 2, "payer + oracle_authority signers");
        assert_eq!(
            vtx.signatures[0],
            Signature::default(),
            "payer slot must remain unsigned for caller"
        );
        assert_ne!(
            vtx.signatures[1],
            Signature::default(),
            "oracle_authority slot must be pre-signed"
        );

        // Once the caller signs slot 0, the full transaction verifies cleanly
        // and passes shillbot_chain::validate_signed_base64.
        let msg_bytes = vtx.message.serialize();
        vtx.signatures[0] = payer.sign_message(&msg_bytes);
        vtx.verify_and_hash_message()
            .expect("both signatures verify over the v0 message");

        let signed_b64 = base64::engine::general_purpose::STANDARD
            .encode(bincode::serialize(&vtx).expect("serializes"));
        let req = shillbot_chain::ValidationRequest {
            action: shillbot_chain::Action::Verify,
            network: "devnet".to_string(),
            wallet: payer.pubkey().to_string(),
            task_pda: task.to_string(),
            escrow_lamports: None,
            content_id: None,
            sponsor: None,
            expected_accounts: None,
            expected_data: None,
            allowed_payout_to: None,
            require_payout_routing: false,
            switchboard_feed: Some(oracle.pubkey().to_string()),
        };
        shillbot_chain::validate_signed_base64(&signed_b64, &req)
            .expect("co-signed verify tx passes shillbot-chain validation");
    }
}

