use crate::errors::McpServiceError;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_SESSIONS_PER_WALLET: usize = 5;
const SESSION_TTL_HOURS: i64 = 24;
const CLAIM_COOLDOWN_SECS: i64 = 60;
const MAX_SUBMITTED_TASKS_PER_SESSION: usize = 1000;

/// A session key that the MCP server holds on behalf of an agent.
/// Scoped to claim_task and submit_work only.
#[derive(Clone, Debug)]
pub struct Session {
    pub wallet_pubkey: String,
    pub session_keypair_bytes: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub last_claim_at: Option<DateTime<Utc>>,
    pub submitted_tasks: Vec<String>,
}

/// Manages session lifecycle: creation, validation, rate limiting, revocation.
pub struct SessionManager {
    sessions: Mutex<HashMap<String, Vec<Session>>>,
}

impl SessionManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
        })
    }

    /// Register a new session for the given wallet.
    /// The session_keypair_bytes are the full 64-byte Ed25519 keypair.
    pub async fn create_session(
        &self,
        wallet_pubkey: &str,
        session_keypair_bytes: Vec<u8>,
    ) -> Result<String, McpServiceError> {
        assert!(!wallet_pubkey.is_empty(), "wallet_pubkey must not be empty");
        assert_eq!(
            session_keypair_bytes.len(),
            64,
            "session keypair must be 64 bytes"
        );

        let mut sessions = self.sessions.lock().await;
        let wallet_sessions = sessions.entry(wallet_pubkey.to_string()).or_default();

        // Evict expired sessions before checking the cap
        Self::evict_expired_sessions(wallet_sessions);

        if wallet_sessions.len() >= MAX_SESSIONS_PER_WALLET {
            return Err(McpServiceError::RateLimited(format!(
                "wallet {wallet_pubkey} has {MAX_SESSIONS_PER_WALLET} active sessions"
            )));
        }

        let session_pubkey = extract_session_pubkey(&session_keypair_bytes);

        wallet_sessions.push(Session {
            wallet_pubkey: wallet_pubkey.to_string(),
            session_keypair_bytes,
            created_at: Utc::now(),
            last_claim_at: None,
            submitted_tasks: Vec::new(),
        });

        assert!(
            wallet_sessions.len() <= MAX_SESSIONS_PER_WALLET,
            "session count must not exceed max"
        );

        Ok(session_pubkey)
    }

    /// Get the session keypair bytes for a wallet, validating it is not expired.
    /// Returns the most recent active session.
    pub async fn get_active_session(
        &self,
        wallet_pubkey: &str,
    ) -> Result<Session, McpServiceError> {
        assert!(!wallet_pubkey.is_empty(), "wallet_pubkey must not be empty");

        let mut sessions = self.sessions.lock().await;
        let wallet_sessions = sessions
            .get_mut(wallet_pubkey)
            .ok_or_else(|| McpServiceError::SessionNotFound(wallet_pubkey.to_string()))?;

        Self::evict_expired_sessions(wallet_sessions);

        let session = wallet_sessions
            .last()
            .ok_or_else(|| McpServiceError::SessionExpired(wallet_pubkey.to_string()))?;

        assert!(
            !is_expired(session),
            "eviction should have removed expired sessions"
        );

        Ok(session.clone())
    }

    /// Check and enforce the claim rate limit (max 1 claim per minute).
    /// Updates the last_claim_at timestamp on success.
    pub async fn check_claim_rate_limit(&self, wallet_pubkey: &str) -> Result<(), McpServiceError> {
        assert!(!wallet_pubkey.is_empty(), "wallet_pubkey must not be empty");

        let mut sessions = self.sessions.lock().await;
        let wallet_sessions = sessions
            .get_mut(wallet_pubkey)
            .ok_or_else(|| McpServiceError::SessionNotFound(wallet_pubkey.to_string()))?;

        let session = wallet_sessions
            .last_mut()
            .ok_or_else(|| McpServiceError::SessionExpired(wallet_pubkey.to_string()))?;

        if let Some(last_claim) = session.last_claim_at {
            let elapsed = Utc::now().signed_duration_since(last_claim).num_seconds();
            if elapsed < CLAIM_COOLDOWN_SECS {
                let wait = CLAIM_COOLDOWN_SECS.saturating_sub(elapsed);
                return Err(McpServiceError::RateLimited(format!(
                    "claim cooldown: wait {wait}s"
                )));
            }
        }

        session.last_claim_at = Some(Utc::now());
        Ok(())
    }

    /// Check and enforce the submission rate limit (max 1 submission per task).
    /// Records the task_id on success.
    pub async fn check_submit_rate_limit(
        &self,
        wallet_pubkey: &str,
        task_id: &str,
    ) -> Result<(), McpServiceError> {
        assert!(!wallet_pubkey.is_empty(), "wallet_pubkey must not be empty");
        assert!(!task_id.is_empty(), "task_id must not be empty");

        let mut sessions = self.sessions.lock().await;
        let wallet_sessions = sessions
            .get_mut(wallet_pubkey)
            .ok_or_else(|| McpServiceError::SessionNotFound(wallet_pubkey.to_string()))?;

        let session = wallet_sessions
            .last_mut()
            .ok_or_else(|| McpServiceError::SessionExpired(wallet_pubkey.to_string()))?;

        if session.submitted_tasks.contains(&task_id.to_string()) {
            return Err(McpServiceError::RateLimited(format!(
                "already submitted for task {task_id}"
            )));
        }

        if session.submitted_tasks.len() >= MAX_SUBMITTED_TASKS_PER_SESSION {
            return Err(McpServiceError::RateLimited(
                "maximum submissions per session reached".to_string(),
            ));
        }

        session.submitted_tasks.push(task_id.to_string());

        assert!(
            session.submitted_tasks.contains(&task_id.to_string()),
            "task must be recorded after submission"
        );

        Ok(())
    }

    /// Revoke all sessions for a wallet.
    pub async fn revoke_sessions(&self, wallet_pubkey: &str) {
        let mut sessions = self.sessions.lock().await;
        sessions.remove(wallet_pubkey);
    }

    fn evict_expired_sessions(wallet_sessions: &mut Vec<Session>) {
        wallet_sessions.retain(|s| !is_expired(s));
    }
}

fn is_expired(session: &Session) -> bool {
    let age = Utc::now().signed_duration_since(session.created_at);
    age.num_hours() >= SESSION_TTL_HOURS
}

/// Extract the public key (bs58) from a 64-byte Ed25519 keypair.
fn extract_session_pubkey(keypair_bytes: &[u8]) -> String {
    assert!(
        keypair_bytes.len() >= 64,
        "keypair must be at least 64 bytes"
    );
    bs58::encode(&keypair_bytes[32..64]).into_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn make_test_keypair() -> Vec<u8> {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(signing_key.as_bytes());
        bytes.extend_from_slice(signing_key.verifying_key().as_bytes());
        bytes
    }

    #[tokio::test]
    async fn test_create_session() {
        let manager = SessionManager::new();
        let keypair = make_test_keypair();

        let result = manager.create_session("wallet1", keypair).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_max_sessions_per_wallet() {
        let manager = SessionManager::new();

        for i in 0..MAX_SESSIONS_PER_WALLET {
            let keypair = make_test_keypair();
            let result = manager.create_session("wallet1", keypair).await;
            assert!(result.is_ok(), "session {i} should succeed");
        }

        let keypair = make_test_keypair();
        let result = manager.create_session("wallet1", keypair).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            McpServiceError::RateLimited(_)
        ));
    }

    #[tokio::test]
    async fn test_get_active_session() {
        let manager = SessionManager::new();
        let keypair = make_test_keypair();
        manager.create_session("wallet1", keypair).await.unwrap();

        let session = manager.get_active_session("wallet1").await;
        assert!(session.is_ok());
        assert_eq!(session.unwrap().wallet_pubkey, "wallet1");
    }

    #[tokio::test]
    async fn test_get_active_session_not_found() {
        let manager = SessionManager::new();
        let result = manager.get_active_session("missing").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            McpServiceError::SessionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_claim_rate_limit() {
        let manager = SessionManager::new();
        let keypair = make_test_keypair();
        manager.create_session("wallet1", keypair).await.unwrap();

        let first_claim = manager.check_claim_rate_limit("wallet1").await;
        assert!(first_claim.is_ok());

        let second_claim = manager.check_claim_rate_limit("wallet1").await;
        assert!(second_claim.is_err());
        assert!(matches!(
            second_claim.unwrap_err(),
            McpServiceError::RateLimited(_)
        ));
    }

    #[tokio::test]
    async fn test_submit_rate_limit_per_task() {
        let manager = SessionManager::new();
        let keypair = make_test_keypair();
        manager.create_session("wallet1", keypair).await.unwrap();

        let first = manager.check_submit_rate_limit("wallet1", "task1").await;
        assert!(first.is_ok());

        let duplicate = manager.check_submit_rate_limit("wallet1", "task1").await;
        assert!(duplicate.is_err());
        assert!(matches!(
            duplicate.unwrap_err(),
            McpServiceError::RateLimited(_)
        ));

        let different_task = manager.check_submit_rate_limit("wallet1", "task2").await;
        assert!(different_task.is_ok());
    }

    #[tokio::test]
    async fn test_revoke_sessions() {
        let manager = SessionManager::new();
        let keypair = make_test_keypair();
        manager.create_session("wallet1", keypair).await.unwrap();

        manager.revoke_sessions("wallet1").await;

        let result = manager.get_active_session("wallet1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_revoke_nonexistent_wallet_is_noop() {
        let manager = SessionManager::new();
        // Should not panic or error
        manager.revoke_sessions("nonexistent").await;
    }

    #[tokio::test]
    async fn test_submitted_tasks_cap() {
        let manager = SessionManager::new();
        let keypair = make_test_keypair();
        manager.create_session("wallet1", keypair).await.unwrap();

        // Submit up to the cap
        for i in 0..MAX_SUBMITTED_TASKS_PER_SESSION {
            let task_id = format!("task_{i}");
            let result = manager.check_submit_rate_limit("wallet1", &task_id).await;
            assert!(result.is_ok(), "submission {i} should succeed");
        }

        // The next one should be rejected
        let result = manager
            .check_submit_rate_limit("wallet1", "one_too_many")
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            McpServiceError::RateLimited(_)
        ));
    }

    #[tokio::test]
    async fn test_different_wallets_independent_sessions() {
        let manager = SessionManager::new();

        let keypair1 = make_test_keypair();
        manager.create_session("wallet_a", keypair1).await.unwrap();

        let keypair2 = make_test_keypair();
        manager.create_session("wallet_b", keypair2).await.unwrap();

        // Revoking one should not affect the other
        manager.revoke_sessions("wallet_a").await;

        assert!(manager.get_active_session("wallet_a").await.is_err());
        assert!(manager.get_active_session("wallet_b").await.is_ok());
    }

    #[test]
    fn test_extract_session_pubkey() {
        let keypair = make_test_keypair();
        let pubkey = extract_session_pubkey(&keypair);
        // The last 32 bytes of the keypair are the public key
        let expected = bs58::encode(&keypair[32..64]).into_string();
        assert_eq!(pubkey, expected);
    }
}
