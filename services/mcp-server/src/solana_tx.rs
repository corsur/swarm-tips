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
}
