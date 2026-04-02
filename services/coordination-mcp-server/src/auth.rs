use crate::errors::McpServiceError;
use rand::RngCore;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const CHALLENGE_TTL: Duration = Duration::from_secs(300);
const CHALLENGE_BYTES: usize = 32;
const MAX_PENDING_CHALLENGES: usize = 10_000;

/// A pending challenge waiting for the agent to sign.
struct PendingChallenge {
    message: Vec<u8>,
    created_at: Instant,
}

/// Manages challenge-response authentication for agent wallets.
///
/// Flow:
/// 1. Agent requests a challenge for their wallet pubkey.
/// 2. Server generates a random challenge message.
/// 3. Agent signs the challenge with their wallet private key.
/// 4. Server verifies the signature against the wallet pubkey.
pub struct ChallengeManager {
    pending: Mutex<HashMap<String, PendingChallenge>>,
}

impl ChallengeManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(HashMap::new()),
        })
    }

    /// Generate a random challenge for the given wallet address.
    /// Returns the challenge bytes (hex-encoded for transport).
    pub async fn create_challenge(&self, wallet_pubkey: &str) -> Result<String, McpServiceError> {
        assert!(!wallet_pubkey.is_empty(), "wallet_pubkey must not be empty");

        let mut pending = self.pending.lock().await;

        // Evict expired challenges to bound memory
        Self::evict_expired(&mut pending);

        if pending.len() >= MAX_PENDING_CHALLENGES {
            return Err(McpServiceError::RateLimited(
                "too many pending challenges".to_string(),
            ));
        }

        let mut challenge_bytes = vec![0u8; CHALLENGE_BYTES];
        rand::thread_rng().fill_bytes(&mut challenge_bytes);

        let challenge_hex = hex::encode(&challenge_bytes);

        let message = build_challenge_message(wallet_pubkey, &challenge_hex);

        pending.insert(
            wallet_pubkey.to_string(),
            PendingChallenge {
                message: message.clone(),
                created_at: Instant::now(),
            },
        );

        assert!(
            pending.contains_key(wallet_pubkey),
            "challenge must be stored after creation"
        );

        Ok(challenge_hex)
    }

    /// Verify that the agent signed the pending challenge with the claimed wallet.
    /// Returns Ok(()) if the signature is valid, consuming the challenge.
    pub async fn verify_challenge(
        &self,
        wallet_pubkey: &str,
        challenge_hex: &str,
        signature_bs58: &str,
    ) -> Result<(), McpServiceError> {
        assert!(!wallet_pubkey.is_empty(), "wallet_pubkey must not be empty");

        let mut pending = self.pending.lock().await;

        let challenge = pending.remove(wallet_pubkey).ok_or_else(|| {
            tracing::warn!(
                service = "shillbot-mcp-server",
                wallet = %wallet_pubkey,
                "auth rejected: no pending challenge"
            );
            McpServiceError::AuthFailed(format!("no pending challenge for wallet {wallet_pubkey}"))
        })?;

        if challenge.created_at.elapsed() > CHALLENGE_TTL {
            tracing::warn!(
                service = "shillbot-mcp-server",
                wallet = %wallet_pubkey,
                "auth rejected: challenge expired"
            );
            return Err(McpServiceError::AuthFailed("challenge expired".to_string()));
        }

        let expected_message = build_challenge_message(wallet_pubkey, challenge_hex);
        if challenge.message != expected_message {
            tracing::warn!(
                service = "shillbot-mcp-server",
                wallet = %wallet_pubkey,
                "auth rejected: challenge message mismatch"
            );
            return Err(McpServiceError::AuthFailed(
                "challenge message mismatch".to_string(),
            ));
        }

        verify_ed25519_signature(wallet_pubkey, &challenge.message, signature_bs58).map_err(
            |e| {
                tracing::warn!(
                    service = "shillbot-mcp-server",
                    wallet = %wallet_pubkey,
                    error = %e,
                    "auth rejected: signature verification failed"
                );
                e
            },
        )?;

        Ok(())
    }

    fn evict_expired(pending: &mut HashMap<String, PendingChallenge>) {
        pending.retain(|_, v| v.created_at.elapsed() <= CHALLENGE_TTL);
    }
}

/// Build the canonical challenge message that the agent must sign.
fn build_challenge_message(wallet_pubkey: &str, challenge_hex: &str) -> Vec<u8> {
    format!("shillbot-mcp-auth:{wallet_pubkey}:{challenge_hex}").into_bytes()
}

/// Verify an Ed25519 signature from a Solana wallet.
fn verify_ed25519_signature(
    wallet_pubkey_bs58: &str,
    message: &[u8],
    signature_bs58: &str,
) -> Result<(), McpServiceError> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let pubkey_bytes = bs58::decode(wallet_pubkey_bs58)
        .into_vec()
        .map_err(|e| McpServiceError::AuthFailed(format!("invalid pubkey: {e}")))?;

    if pubkey_bytes.len() != 32 {
        return Err(McpServiceError::AuthFailed(
            "pubkey must be 32 bytes".to_string(),
        ));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&pubkey_bytes);

    let verifying_key = VerifyingKey::from_bytes(&key_array)
        .map_err(|e| McpServiceError::AuthFailed(format!("invalid ed25519 key: {e}")))?;

    let sig_bytes = bs58::decode(signature_bs58)
        .into_vec()
        .map_err(|e| McpServiceError::AuthFailed(format!("invalid signature encoding: {e}")))?;

    if sig_bytes.len() != 64 {
        return Err(McpServiceError::AuthFailed(
            "signature must be 64 bytes".to_string(),
        ));
    }

    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&sig_bytes);

    let signature = Signature::from_bytes(&sig_array);

    verifying_key
        .verify(message, &signature)
        .map_err(|e| McpServiceError::AuthFailed(format!("signature verification failed: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn generate_test_keypair() -> (SigningKey, String) {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let pubkey_bs58 = bs58::encode(signing_key.verifying_key().as_bytes()).into_string();
        (signing_key, pubkey_bs58)
    }

    #[tokio::test]
    async fn test_create_challenge_returns_hex() {
        let manager = ChallengeManager::new();
        let (_, pubkey) = generate_test_keypair();

        let challenge = manager.create_challenge(&pubkey).await;
        assert!(challenge.is_ok());

        let hex_str = challenge.unwrap();
        assert_eq!(hex_str.len(), CHALLENGE_BYTES * 2);
        assert!(hex::decode(&hex_str).is_ok());
    }

    #[tokio::test]
    async fn test_verify_challenge_valid_signature() {
        let manager = ChallengeManager::new();
        let (signing_key, pubkey) = generate_test_keypair();

        let challenge_hex = manager.create_challenge(&pubkey).await.unwrap();

        let message = build_challenge_message(&pubkey, &challenge_hex);
        let signature = signing_key.sign(&message);
        let sig_bs58 = bs58::encode(signature.to_bytes()).into_string();

        let result = manager
            .verify_challenge(&pubkey, &challenge_hex, &sig_bs58)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_verify_challenge_wrong_signature() {
        let manager = ChallengeManager::new();
        let (_, pubkey) = generate_test_keypair();
        let (wrong_key, _) = generate_test_keypair();

        let challenge_hex = manager.create_challenge(&pubkey).await.unwrap();

        let message = build_challenge_message(&pubkey, &challenge_hex);
        let wrong_sig = wrong_key.sign(&message);
        let sig_bs58 = bs58::encode(wrong_sig.to_bytes()).into_string();

        let result = manager
            .verify_challenge(&pubkey, &challenge_hex, &sig_bs58)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_challenge_no_pending() {
        let manager = ChallengeManager::new();
        let (_, pubkey) = generate_test_keypair();

        let result = manager
            .verify_challenge(&pubkey, "deadbeef", "fakesig")
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            McpServiceError::AuthFailed(_)
        ));
    }

    #[tokio::test]
    async fn test_challenge_consumed_after_verify() {
        let manager = ChallengeManager::new();
        let (signing_key, pubkey) = generate_test_keypair();

        let challenge_hex = manager.create_challenge(&pubkey).await.unwrap();
        let message = build_challenge_message(&pubkey, &challenge_hex);
        let signature = signing_key.sign(&message);
        let sig_bs58 = bs58::encode(signature.to_bytes()).into_string();

        let first = manager
            .verify_challenge(&pubkey, &challenge_hex, &sig_bs58)
            .await;
        assert!(first.is_ok());

        let second = manager
            .verify_challenge(&pubkey, &challenge_hex, &sig_bs58)
            .await;
        assert!(second.is_err());
    }

    #[tokio::test]
    async fn test_new_challenge_replaces_previous_for_same_wallet() {
        let manager = ChallengeManager::new();
        let (signing_key, pubkey) = generate_test_keypair();

        let first_hex = manager.create_challenge(&pubkey).await.unwrap();
        let _second_hex = manager.create_challenge(&pubkey).await.unwrap();

        // The stored challenge should be the second one, so verifying with the
        // first challenge's hex should fail (message mismatch after removal).
        let first_msg = build_challenge_message(&pubkey, &first_hex);
        let first_sig = signing_key.sign(&first_msg);
        let first_sig_bs58 = bs58::encode(first_sig.to_bytes()).into_string();
        let first_result = manager
            .verify_challenge(&pubkey, &first_hex, &first_sig_bs58)
            .await;
        assert!(first_result.is_err(), "old challenge hex should not verify");
    }

    #[tokio::test]
    async fn test_latest_challenge_verifies_after_replacement() {
        let manager = ChallengeManager::new();
        let (signing_key, pubkey) = generate_test_keypair();

        let _first_hex = manager.create_challenge(&pubkey).await.unwrap();
        let second_hex = manager.create_challenge(&pubkey).await.unwrap();

        // The second (latest) challenge should verify successfully
        let second_msg = build_challenge_message(&pubkey, &second_hex);
        let second_sig = signing_key.sign(&second_msg);
        let second_sig_bs58 = bs58::encode(second_sig.to_bytes()).into_string();
        let second_result = manager
            .verify_challenge(&pubkey, &second_hex, &second_sig_bs58)
            .await;
        assert!(second_result.is_ok(), "latest challenge should verify");
    }

    #[test]
    fn test_verify_invalid_pubkey_encoding() {
        let result = verify_ed25519_signature("not-valid-bs58!!!", b"hello", "fakesig");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("invalid pubkey") || err_msg.contains("invalid signature"),
            "error should indicate encoding problem: {err_msg}"
        );
    }

    #[test]
    fn test_verify_pubkey_wrong_length() {
        // Valid bs58 but wrong length (only 16 bytes)
        let short_key = bs58::encode(&[0u8; 16]).into_string();
        let result = verify_ed25519_signature(&short_key, b"hello", "fakesig");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("pubkey must be 32 bytes"));
    }

    #[test]
    fn test_verify_signature_wrong_length() {
        let (_, pubkey) = generate_test_keypair();
        // Valid bs58 but wrong length (only 32 bytes, needs 64)
        let short_sig = bs58::encode(&[0u8; 32]).into_string();
        let result = verify_ed25519_signature(&pubkey, b"hello", &short_sig);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("signature must be 64 bytes"));
    }

    #[test]
    fn test_build_challenge_message_format() {
        let msg = build_challenge_message("abc123", "deadbeef");
        assert_eq!(msg, b"shillbot-mcp-auth:abc123:deadbeef");
    }
}
