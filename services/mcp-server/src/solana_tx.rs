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
}
