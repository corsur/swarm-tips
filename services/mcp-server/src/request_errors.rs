//! Recovery guidance and content-free diagnostics at the request boundary.
use rmcp::ErrorData;
use serde_json::{json, Value};
use tracing::Instrument;

#[derive(Clone)]
pub(crate) struct ErrorContext {
    pub operation: String,
    pub request_id: String,
    pub transport: &'static str,
    pub read_only: bool,
}

tokio::task_local! {
    pub(crate) static CONTEXT: ErrorContext;
}

impl ErrorContext {
    pub fn new(operation: &str, transport: &'static str, read_only: bool) -> Self {
        Self {
            operation: operation.to_owned(),
            request_id: uuid::Uuid::new_v4().to_string(),
            transport,
            read_only,
        }
    }

    pub fn failure(&self, code: &str, stage: &str) {
        // Deliberately exclude arguments, error prose, wallet/session identities,
        // resource URIs and caller-provided request IDs.
        tracing::warn!(
            event = "request_failed",
            operation = %self.operation,
            request_id = %self.request_id,
            transport = self.transport,
            error_code = code,
            stage,
            "request failed; use request_id to correlate recovery guidance"
        );
    }
}

pub(crate) fn current() -> Option<ErrorContext> {
    CONTEXT.try_with(Clone::clone).ok()
}

pub(crate) const OPEN_EXAMPLE: &str = r#"Use {"messages":[{"msg_id":"ID_FROM_LIST","direction":"received"}]}; get IDs from agent_list_messages. Opening does not acknowledge."#;

/// Preserve established reason tokens; recovery fields are additive.
pub(crate) fn recovery(reason: &str, read_only: bool) -> (&'static str, &'static str) {
    match reason {
        "unproven_sender" => ("after_correction", "Sign the nonce from register_wallet locally, then call register_wallet with pubkey, nonce and signature in this same session. For team support without verification, omit to_wallet in agent_send_message."),
        "missing_session" => ("after_correction", "MCP: initialize and retain Mcp-Session-Id. HTTP: obtain X-Inbox-Session via POST /internal/inbox/session and send it on subsequent requests."),
        "invalid_request" | "invalid_arguments" => ("after_correction", "Correct the request before retrying. For tools, inspect its inputSchema in tools/list; retain the same session."),
        "unknown_tool" => ("after_correction", "Call tools/list for this endpoint. Use list_related_servers to find a focused endpoint with the needed catalog; initialize a separate session there."),
        "unknown_resource" => ("after_correction", "Call resources/list and copy a returned URI exactly. Resources are available on https://mcp.swarm.tips/mcp."),
        "timeout" if read_only => ("safe_read", "This read timed out. Retry with backoff; it did not request a state change."),
        "timeout" => ("reconcile_first", "Completion is unknown. Do not repeat a write or sign a replacement transaction. Inspect the relevant task/game state and any transaction signature first. For Shillbot, use shillbot_get_task_details and shillbot_confirm_tx for an already-landed transaction."),
        "internal_error" | "internal" if read_only => ("safe_read", "Retry this read with backoff. If it persists, give support the request_id."),
        "internal_error" | "internal" => ("reconcile_first", "Do not automatically repeat this operation. Check its state and any transaction signature first; give support the request_id if unresolved."),
        _ => ("after_correction", "Follow the error explanation before retrying; contact support with request_id if unresolved."),
    }
}

pub(crate) fn decorate(mut error: ErrorData, ctx: &ErrorContext) -> ErrorData {
    let schema_failure = error
        .message
        .starts_with("failed to deserialize parameters:");
    let reason = if schema_failure {
        "invalid_arguments".to_owned()
    } else if error.message.contains("timed out") {
        "timeout".to_owned()
    } else if let Some(reason) = error
        .data
        .as_ref()
        .and_then(|d| d.get("reason"))
        .and_then(Value::as_str)
    {
        reason.to_owned()
    } else if error.code == rmcp::model::ErrorCode::INVALID_PARAMS {
        "invalid_request".to_owned()
    } else {
        "internal_error".to_owned()
    };
    let (retry, mut next) = recovery(&reason, ctx.read_only);
    if schema_failure {
        // Serde errors can echo submitted message text or signatures. Do not
        // return them to rmcp's generic error logger.
        error.message = "Arguments do not match this tool's inputSchema.".into();
        if ctx.operation == "agent_open_messages" {
            next = OPEN_EXAMPLE;
        } else if ctx.operation == "register_wallet" {
            next = r#"Start with {"pubkey":"YOUR_PUBLIC_WALLET_ADDRESS"}. Sign the returned nonce locally and repeat register_wallet with pubkey, nonce and signature in this same session. Never send a private key."#;
        }
    } else if error.message == "include_sent requires status=all" {
        next = "Set status=all with include_sent=true to browse sent history; use include_sent=false for the pending inbox.";
    }
    // Some MCP clients display only Error.message, not Error.data. Keep the
    // recovery action and reference usable on those clients too.
    error.message = format!(
        "{} Next step: {next} Reference: {}",
        error.message, ctx.request_id
    )
    .into();
    let mut data = match error.data.take() {
        Some(Value::Object(data)) => data,
        _ => serde_json::Map::new(),
    };
    data.extend(
        json!({
            "error_code": reason,
            "operation": ctx.operation,
            "request_id": ctx.request_id,
            "retry": retry,
            "next_step": next,
        })
        .as_object()
        .cloned()
        .unwrap_or_default(),
    );
    error.data = Some(Value::Object(data));
    ctx.failure(
        &reason,
        if schema_failure {
            "arguments"
        } else {
            "handler"
        },
    );
    error
}

/// Apply context before Axum extraction, so malformed bodies/queries also get
/// an operation and correlation reference. Only constant routes enter logs.
pub(crate) async fn observe_http(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let operation = match request.uri().path() {
        "/internal/inbox/list" => "agent_list_messages",
        "/internal/inbox/open" => "agent_open_messages",
        "/internal/inbox/ack-ids" => "agent_ack_message_ids",
        "/internal/inbox/messages" => "agent_get_messages",
        "/internal/inbox/send" => "agent_send_message",
        "/internal/inbox/ack" => "agent_ack_messages",
        "/internal/inbox/session" => "inbox_session",
        "/internal/inbox/webhook" => "inbox_webhook",
        "/internal/topics/publish" => "agent_publish_topic",
        "/internal/topics/read" => "agent_read_topic",
        "/internal/topics/report" => "agent_report_topic",
        "/a2a" => "a2a",
        _ => "inbox_http",
    };
    let read_only = request.method() == http::Method::GET || operation == "agent_open_messages";
    let ctx = ErrorContext::new(operation, "http", read_only);
    let span = tracing::info_span!("inbox_request", operation = %ctx.operation, request_id = %ctx.request_id);
    CONTEXT
        .scope(ctx.clone(), async move {
            let mut response = next.run(request).await;
            if response.status().is_client_error() || response.status().is_server_error() {
                // json_error logs specific codes; extractor failures have no marker.
                if !response.headers().contains_key("x-request-id") {
                    ctx.failure("http_rejected", "extraction");
                    if let Ok(value) = http::HeaderValue::from_str(&ctx.request_id) {
                        response.headers_mut().insert("x-request-id", value);
                    }
                }
            }
            response
        })
        .instrument(span)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_failure_does_not_echo_content_and_shows_message_reference_shape() {
        let ctx = ErrorContext::new("agent_open_messages", "mcp", true);
        let error = decorate(
            ErrorData::invalid_params("failed to deserialize parameters: secret-body", None),
            &ctx,
        );
        let wire = serde_json::to_string(&error).unwrap();
        assert!(!wire.contains("secret-body"));
        assert!(wire.contains("msg_id"));
        assert!(error.message.contains("msg_id"));
        assert!(error.message.contains(&ctx.request_id));
        assert_eq!(error.data.as_ref().unwrap()["request_id"], ctx.request_id);
        assert_eq!(error.data.unwrap()["error_code"], "invalid_arguments");
    }

    #[test]
    fn timeout_retry_policy_distinguishes_reads_from_uncertain_writes() {
        for (read_only, expected) in [(true, "safe_read"), (false, "reconcile_first")] {
            let ctx = ErrorContext::new("test", "mcp", read_only);
            let error = decorate(
                ErrorData::internal_error("module request timed out; completion is unknown", None),
                &ctx,
            );
            assert_eq!(error.data.as_ref().unwrap()["retry"], expected);
            assert_eq!(error.data.unwrap()["error_code"], "timeout");
        }
    }

    #[test]
    fn list_failure_preserves_reason_and_explains_exact_correction() {
        let ctx = ErrorContext::new("agent_list_messages", "mcp", true);
        let error = decorate(
            ErrorData::invalid_params(
                "include_sent requires status=all",
                Some(json!({"reason":"invalid_request"})),
            ),
            &ctx,
        );
        let data = error.data.unwrap();
        assert_eq!(data["reason"], "invalid_request");
        assert!(data["next_step"].as_str().unwrap().contains("status=all"));
    }

    #[tokio::test]
    async fn malformed_http_json_gets_operation_context_and_a_reference_before_dispatch() {
        async fn handler(_: axum::Json<Value>) -> axum::http::StatusCode {
            panic!("malformed JSON must not reach the handler");
        }
        let app = axum::Router::new().route(
            "/internal/inbox/open",
            axum::routing::post(handler).layer(axum::middleware::from_fn(observe_http)),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/internal/inbox/open",
            listener.local_addr().unwrap()
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = reqwest::Client::new()
            .post(url)
            .header("content-type", "application/json")
            .body("{")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
        assert!(
            uuid::Uuid::parse_str(response.headers()["x-request-id"].to_str().unwrap()).is_ok()
        );
        server.abort();
    }

    #[tokio::test]
    async fn concurrent_request_contexts_are_isolated() {
        let a = ErrorContext::new("agent_open_messages", "mcp", true);
        let b = ErrorContext::new("agent_send_message", "http", false);
        let (left, right) = tokio::join!(
            CONTEXT.scope(a.clone(), async {
                tokio::task::yield_now().await;
                current().unwrap()
            }),
            CONTEXT.scope(b.clone(), async {
                tokio::task::yield_now().await;
                current().unwrap()
            }),
        );
        assert_eq!(left.request_id, a.request_id);
        assert_eq!(right.operation, b.operation);
        assert_ne!(left.request_id, right.request_id);
        assert!(current().is_none());
    }
}
