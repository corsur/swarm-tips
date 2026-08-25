//! Minimal Google Workflows `executions.create` trigger.
//!
//! Mirrors `coordination-app/crates/gcp-workflows::execute_workflow` (the two
//! repos share by contract, not source — same reason `config.rs` carries its
//! own Secret Manager helper). Auth is the runtime metadata server (Cloud Run
//! service account, `roles/workflows.invoker`) — no keys, no env secrets.
//!
//! Used by `inbox::Inbox` to start the durable `agent-webhook-delivery`
//! workflow (coordination-app owns the workflow YAML + Terraform). Per the
//! cross-repo Workflows standard, delivery retries/backoff/dead-lettering
//! live in the workflow, never in-process.

use anyhow::Context;

/// Default metadata-server token endpoint (overridable for wiremock tests).
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
const WORKFLOWS_API_BASE: &str = "https://workflowexecutions.googleapis.com";

pub struct WorkflowsTrigger {
    project: String,
    location: String,
    token_url: String,
    api_base: String,
    http: reqwest::Client,
}

impl WorkflowsTrigger {
    pub fn new(project: String, location: String) -> Self {
        assert!(!project.is_empty(), "project must not be empty");
        assert!(!location.is_empty(), "location must not be empty");
        Self {
            project,
            location,
            token_url: METADATA_TOKEN_URL.to_string(),
            api_base: WORKFLOWS_API_BASE.to_string(),
            http: reqwest::Client::new(),
        }
    }

    #[cfg(test)]
    fn with_endpoints(project: &str, location: &str, token_url: &str, api_base: &str) -> Self {
        Self {
            project: project.to_string(),
            location: location.to_string(),
            token_url: token_url.to_string(),
            api_base: api_base.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Access token from the metadata server. Off GCP this fails fast (no
    /// metadata host) — callers degrade gracefully (log + skip).
    async fn access_token(&self) -> anyhow::Result<String> {
        let resp = self
            .http
            .get(&self.token_url)
            .header("Metadata-Flavor", "Google")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .context("metadata server unreachable")?;
        let status = resp.status();
        if !status.is_success() {
            let body = match resp.text().await {
                Ok(b) => b,
                Err(e) => format!("<body unreadable: {e}>"),
            };
            anyhow::bail!("metadata server returned {status}: {body}");
        }
        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
        }
        let token: TokenResponse = resp.json().await.context("parse metadata token")?;
        Ok(token.access_token)
    }

    /// Start one workflow execution. The Workflows API takes the argument as
    /// a JSON STRING under `"argument"` — `args` is serialized once here.
    /// Returns the execution name.
    pub async fn execute(
        &self,
        workflow: &str,
        args: &serde_json::Value,
    ) -> anyhow::Result<String> {
        assert!(!workflow.is_empty(), "workflow must not be empty");
        let url = format!(
            "{}/v1/projects/{}/locations/{}/workflows/{}/executions",
            self.api_base, self.project, self.location, workflow
        );
        let body = serde_json::json!({ "argument": args.to_string() });
        let token = self.access_token().await?;
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .timeout(std::time::Duration::from_secs(15))
            .json(&body)
            .send()
            .await
            .context("workflows executions.create request")?;
        let status = resp.status();
        let resp_body = match resp.text().await {
            Ok(b) => b,
            Err(e) => format!("<body unreadable: {e}>"),
        };
        if !status.is_success() {
            anyhow::bail!("workflows API returned {status}: {resp_body}");
        }
        let execution = serde_json::from_str::<serde_json::Value>(&resp_body)
            .ok()
            .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string());
        // Postcondition: a started execution is observable in the logs.
        debug_assert!(!execution.is_empty(), "execution name never empty");
        tracing::info!(workflow, execution = %execution, "workflow execution started");
        Ok(execution)
    }
}

// ---------------------------------------------------------------------------
// Tests — wiremock the metadata server + the executions.create endpoint.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mock_metadata(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/token"))
            .and(header("Metadata-Flavor", "Google"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "access_token": "tok-1" })),
            )
            .expect(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn execute_posts_the_stringified_argument_shape() {
        // The contract with agent-webhook-delivery: argument is a JSON STRING
        // (not a nested object) carrying the delivery fields.
        let server = MockServer::start().await;
        mock_metadata(&server).await;
        Mock::given(method("POST"))
            .and(path(
                "/v1/projects/p1/locations/us-central1/workflows/agent-webhook-delivery/executions",
            ))
            .and(header("Authorization", "Bearer tok-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "projects/p1/locations/us-central1/workflows/agent-webhook-delivery/executions/e1",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let trigger = WorkflowsTrigger::with_endpoints(
            "p1",
            "us-central1",
            &format!("{}/token", server.uri()),
            &server.uri(),
        );
        let args = json!({
            "webhook_url": "https://agent.example.com/hook",
            "signature": "sha256=abc",
            "payload_json": "{\"event\":\"inbox_message\"}",
            "delivery_id": "d1",
            "wallet": "solana:x:w",
            "mcp_url": "https://mcp.swarm.tips",
        });
        let execution = trigger
            .execute("agent-webhook-delivery", &args)
            .await
            .expect("execution starts");
        assert!(execution.ends_with("/executions/e1"), "{execution}");

        // The recorded request body must be {"argument": "<json string>"} —
        // a nested object would silently break the workflow's json.decode.
        let requests = server.received_requests().await.expect("requests");
        let create = requests
            .iter()
            .find(|r| r.url.path().ends_with("/executions"))
            .expect("create call recorded");
        let body: serde_json::Value =
            serde_json::from_slice(&create.body).expect("request body is JSON");
        let argument = body["argument"].as_str().expect("argument is a STRING");
        let decoded: serde_json::Value =
            serde_json::from_str(argument).expect("argument decodes back to JSON");
        assert_eq!(decoded, args, "argument round-trips the trigger args");
    }

    #[tokio::test]
    async fn execute_surfaces_api_errors() {
        let server = MockServer::start().await;
        mock_metadata(&server).await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403).set_body_string("PERMISSION_DENIED"))
            .expect(1)
            .mount(&server)
            .await;

        let trigger = WorkflowsTrigger::with_endpoints(
            "p1",
            "us-central1",
            &format!("{}/token", server.uri()),
            &server.uri(),
        );
        let err = trigger
            .execute("agent-webhook-delivery", &json!({}))
            .await
            .expect_err("403 must error");
        let msg = err.to_string();
        assert!(msg.contains("403"), "carries the status: {msg}");
    }

    #[tokio::test]
    async fn missing_metadata_server_degrades_to_error_not_panic() {
        // Off-GCP (local dev): the token fetch fails fast and the caller
        // logs + skips — the send itself must never depend on this.
        let trigger = WorkflowsTrigger::with_endpoints(
            "p1",
            "us-central1",
            "http://127.0.0.1:9/token", // discard port — connection refused
            "http://127.0.0.1:9",
        );
        let err = trigger
            .execute("agent-webhook-delivery", &json!({}))
            .await
            .expect_err("no metadata server");
        assert!(err.to_string().contains("metadata"), "{err}");
    }
}
