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
