//! Runtime config helpers. Non-sensitive values (URLs, port, project id)
//! continue to come from env vars via `load_env_or` in `main.rs`. Sensitive
//! values (API keys) come from GCP Secret Manager at startup, never from env
//! vars, never from K8s Secrets.
//!
//! This is the cross-repo standard — see `swarm/CLAUDE.md` "Direct Secret
//! Manager reads only" and `coordination-app/backend/CLAUDE.md` "Three secret
//! categories, three homes". mcp-server was the last outlier on env-var-only
//! config; this module brings it into compliance.
//!
//! The `load_optional_secret` helper below is lifted verbatim from
//! `coordination-app/backend/x-bridge/src/config.rs::load_optional_secret`
//! (lines 103-144 at the time of copying). Keep them in sync by hand — if
//! one needs an error-handling tweak, apply the same change to both.

use gcloud_sdk::google::cloud::secretmanager::v1::{
    secret_manager_service_client::SecretManagerServiceClient, AccessSecretVersionRequest,
};
use gcloud_sdk::GoogleApi;

/// Load an optional secret from GCP Secret Manager. Returns None on any
/// failure (missing secret, client build error, decode error) and logs a
/// `warn!` with structured fields so the pod can boot in degraded mode.
///
/// Use this for secrets whose absence should disable a feature but not
/// crash-loop the pod (e.g., `xai-api-key` disables the Layer 2 LLM
/// classifier but leaves Layer 1 + Layer 3 intact).
///
/// For required secrets whose absence SHOULD crash-loop the pod, add a
/// `load_secret` helper here that panics on failure — same shape as
/// `coordination-app/backend/chatwoot-responder/src/config.rs::load_secret`.
pub async fn load_optional_secret(project_id: &str, secret_name: &str) -> Option<String> {
    let client: GoogleApi<SecretManagerServiceClient<_>> = match GoogleApi::from_function(
        SecretManagerServiceClient::new,
        "https://secretmanager.googleapis.com",
        None,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                service = "mcp-server",
                secret = secret_name,
                error = %e,
                "failed to create Secret Manager client for optional secret"
            );
            return None;
        }
    };

    let name = format!("projects/{project_id}/secrets/{secret_name}/versions/latest");

    match client
        .get()
        .access_secret_version(AccessSecretVersionRequest { name })
        .await
    {
        Ok(resp) => {
            let payload = resp.into_inner().payload?;
            String::from_utf8(payload.data.ref_sensitive_value().to_vec()).ok()
        }
        Err(e) => {
            tracing::warn!(
                service = "mcp-server",
                secret = secret_name,
                error = %e,
                "optional secret not found in Secret Manager"
            );
            None
        }
    }
}
