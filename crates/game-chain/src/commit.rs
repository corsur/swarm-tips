//! Commit-reveal secret generation for the coordination game.
//!
//! The commit scheme encodes the player's guess (0 = "same team",
//! 1 = "different team") in the low bit of a 32-byte random preimage.
//! The commitment is SHA-256 of the preimage. On reveal, the on-chain
//! program recovers the guess from `preimage[31] & 1` and verifies
//! `SHA-256(preimage) == commitment`.

use sha2::{Digest, Sha256};

/// Error type for commit operations.
#[derive(Debug, thiserror::Error)]
pub enum CommitError {
    /// Guess value must be 0 or 1.
    #[error("guess must be 0 or 1, got {0}")]
    InvalidGuess(u8),
}

/// Generate a random 32-byte preimage encoding `guess` in the low bit,
/// and compute the SHA-256 commitment.
///
/// Returns `(preimage, commitment)` where:
/// - `preimage[31] & 1 == guess`
/// - `commitment == SHA-256(preimage)`
///
/// # Errors
///
/// Returns `CommitError::InvalidGuess` if `guess > 1`.
pub fn generate_commit_secret(guess: u8) -> Result<([u8; 32], [u8; 32]), CommitError> {
    if guess > 1 {
        return Err(CommitError::InvalidGuess(guess));
    }

    use rand::RngCore;
    let mut preimage = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut preimage);

    // Encode guess in the low bit of the last byte.
    preimage[31] = (preimage[31] & 0xFE) | guess;

    let commitment: [u8; 32] = Sha256::digest(preimage).into();

    // Postcondition: the guess round-trips through the last bit.
    assert_eq!(
        preimage[31] & 1,
        guess,
        "preimage must encode guess in low bit"
    );
    // Postcondition: commitment is a valid SHA-256 hash (non-zero).
    assert_ne!(commitment, [0u8; 32], "commitment must be non-zero");

    Ok((preimage, commitment))
}

/// Verify that a preimage matches a commitment and extract the encoded guess.
///
/// Returns the guess value (0 or 1) if verification succeeds.
///
/// This is useful for testing and for verifying stored preimages before
/// submitting a reveal transaction.
pub fn verify_commitment(preimage: &[u8; 32], commitment: &[u8; 32]) -> Option<u8> {
    let recomputed: [u8; 32] = Sha256::digest(preimage).into();
    if recomputed != *commitment {
        return None;
    }
    Some(preimage[31] & 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_commit_secret_guess_zero_encodes_low_bit_as_zero() {
        let (preimage, commitment) = generate_commit_secret(0).expect("guess=0 should succeed");
        assert_eq!(preimage[31] & 1, 0, "guess=0 must clear R[31] low bit");
        let expected: [u8; 32] = Sha256::digest(preimage).into();
        assert_eq!(commitment, expected, "commitment must be SHA-256(preimage)");
    }

    #[test]
    fn generate_commit_secret_guess_one_encodes_low_bit_as_one() {
        let (preimage, commitment) = generate_commit_secret(1).expect("guess=1 should succeed");
        assert_eq!(preimage[31] & 1, 1, "guess=1 must set R[31] low bit");
        let expected: [u8; 32] = Sha256::digest(preimage).into();
        assert_eq!(commitment, expected, "commitment must be SHA-256(preimage)");
    }

    #[test]
    fn generate_commit_secret_produces_distinct_preimages() {
        let (r1, _) = generate_commit_secret(0).expect("should succeed");
        let (r2, _) = generate_commit_secret(0).expect("should succeed");
        assert_ne!(r1, r2, "two calls should produce different preimages");
    }

    #[test]
    fn generate_commit_secret_commitment_changes_with_guess() {
        let (r0, c0) = generate_commit_secret(0).expect("should succeed");
        // Manually flip the guess bit to simulate guess=1 on the same random bytes.
        let mut r1 = r0;
        r1[31] = (r1[31] & 0xFE) | 1;
        let c1: [u8; 32] = Sha256::digest(r1).into();
        assert_ne!(
            c0, c1,
            "different guess bit must produce different commitment"
        );
    }

    #[test]
    fn generate_commit_secret_rejects_invalid_guess() {
        let err = generate_commit_secret(2).unwrap_err();
        assert!(
            err.to_string().contains("guess must be 0 or 1"),
            "expected InvalidGuess error, got: {err}",
        );
    }

    #[test]
    fn generate_commit_secret_rejects_guess_255() {
        let err = generate_commit_secret(255).unwrap_err();
        assert!(
            err.to_string().contains("guess must be 0 or 1"),
            "expected InvalidGuess error, got: {err}",
        );
    }

    #[test]
    fn generate_commit_secret_preimage_is_32_bytes() {
        let (preimage, commitment) = generate_commit_secret(0).expect("should succeed");
        assert_eq!(preimage.len(), 32);
        assert_eq!(commitment.len(), 32);
    }

    #[test]
    fn verify_commitment_succeeds_for_valid_pair() {
        for guess in [0u8, 1] {
            let (preimage, commitment) = generate_commit_secret(guess).expect("should succeed");
            let recovered = verify_commitment(&preimage, &commitment);
            assert_eq!(recovered, Some(guess), "verify must recover guess={guess}");
        }
    }

    #[test]
    fn verify_commitment_fails_for_tampered_preimage() {
        let (mut preimage, commitment) = generate_commit_secret(0).expect("should succeed");
        preimage[0] ^= 0xFF; // Corrupt a byte.
        assert_eq!(
            verify_commitment(&preimage, &commitment),
            None,
            "tampered preimage must not verify"
        );
    }

    #[test]
    fn verify_commitment_fails_for_tampered_commitment() {
        let (preimage, mut commitment) = generate_commit_secret(0).expect("should succeed");
        commitment[0] ^= 0xFF;
        assert_eq!(
            verify_commitment(&preimage, &commitment),
            None,
            "tampered commitment must not verify"
        );
    }
}
