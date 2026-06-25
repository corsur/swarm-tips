//! Operator-side certificate co-signing (backend only).
//!
//! The matchmaker/operator proposes a match-live certificate and co-signs
//! its digest; players co-sign too. This module computes the canonical
//! digest (via `cert_schema`) and produces a 65-byte `[r || s || v]`
//! secp256k1 signature — the exact form the Anchor program's
//! `secp256k1_recover` and the EVM `ecrecover` both verify. Never compiled
//! into BPF (the `cosign` feature is backend-only).

use crate::cert_schema::{keccak256, Checkpoint, MatchLiveCert, OutcomeCert};
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};

/// Why a signing key was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CosignError {
    BadSecretKey,
    SigningFailed,
    /// A co-signature recovered successfully but to the wrong address — the
    /// signer is not the expected session key.
    SignerMismatch,
    /// A high-s (malleable) signature. Rejected so this leg agrees with the EVM
    /// leg, whose OpenZeppelin `ECDSA.recover` reverts on s > n/2.
    MalleableSignature,
}

/// The 20-byte Ethereum address of a secp256k1 secret key — the
/// `operator_signer` / session-key form recorded on-chain.
pub fn eth_address(secret_key: &[u8; 32]) -> Result<[u8; 20], CosignError> {
    let signing =
        SigningKey::from_bytes(secret_key.into()).map_err(|_| CosignError::BadSecretKey)?;
    Ok(verifying_key_address(signing.verifying_key()))
}

fn verifying_key_address(vk: &VerifyingKey) -> [u8; 20] {
    // Uncompressed SEC1 point is 0x04 || x(32) || y(32); the eth address is
    // keccak256(x || y)[12..].
    let point = vk.to_encoded_point(false);
    let hash = keccak256(&point.as_bytes()[1..]);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&hash[12..]);
    addr
}

/// Sign a 32-byte digest, returning `[r(32) || s(32) || v(1)]` with
/// `v = recovery_id` (0 or 1) — the program/EVM-accepted layout.
pub fn sign_digest(secret_key: &[u8; 32], digest: &[u8; 32]) -> Result<[u8; 65], CosignError> {
    let signing =
        SigningKey::from_bytes(secret_key.into()).map_err(|_| CosignError::BadSecretKey)?;
    let (sig, recid): (Signature, RecoveryId) = signing
        .sign_prehash_recoverable(digest)
        .map_err(|_| CosignError::SigningFailed)?;
    let mut out = [0u8; 65];
    out[..64].copy_from_slice(&sig.to_bytes());
    out[64] = recid.to_byte();
    Ok(out)
}

/// Recover the 20-byte Ethereum address that produced a 65-byte
/// `[r || s || v]` signature over `digest` — the same operation the Anchor
/// program's `secp256k1_recover` and the EVM `ecrecover` perform. The
/// backend uses this to verify players' co-signatures before submitting.
pub fn recover_address(digest: &[u8; 32], sig: &[u8; 65]) -> Result<[u8; 20], CosignError> {
    let recid = RecoveryId::from_byte(sig[64]).ok_or(CosignError::SigningFailed)?;
    let signature = Signature::from_slice(&sig[..64]).map_err(|_| CosignError::SigningFailed)?;
    // Reject high-s (malleable) signatures: normalize_s() returns Some only when
    // the input s was in the upper half order. The EVM leg already rejects these
    // (OZ ECDSA); enforcing it here keeps both legs' acceptance sets identical so
    // no malleated twin is valid on one leg but not the other.
    if signature.normalize_s().is_some() {
        return Err(CosignError::MalleableSignature);
    }
    let vk = VerifyingKey::recover_from_prehash(digest, &signature, recid)
        .map_err(|_| CosignError::SigningFailed)?;
    Ok(verifying_key_address(&vk))
}

/// Co-sign a match-live certificate (used by both players and the operator).
pub fn sign_match_live(
    secret_key: &[u8; 32],
    cert: &MatchLiveCert,
) -> Result<[u8; 65], CosignError> {
    sign_digest(secret_key, &keccak256(&cert.encode()))
}

/// Co-sign an outcome certificate.
pub fn sign_outcome(secret_key: &[u8; 32], oc: &OutcomeCert) -> Result<[u8; 65], CosignError> {
    sign_digest(secret_key, &keccak256(&oc.encode()))
}

/// Co-sign a transcript checkpoint.
pub fn sign_checkpoint(secret_key: &[u8; 32], cp: &Checkpoint) -> Result<[u8; 65], CosignError> {
    sign_digest(secret_key, &keccak256(&cp.encode()))
}

/// Verify a checkpoint co-signed by BOTH session keys, exactly as the on-chain
/// `verify_checkpoint` does: `sigs[0]` must recover to leg A's session key
/// (the Solana leg) and `sigs[1]` to leg B's (the EVM leg), both over the
/// canonical checkpoint digest. The backend relay calls this before
/// storing/relaying a checkpoint; settle assembly calls it before submitting.
/// Returns the digest on success so callers need not recompute it.
pub fn verify_cosigned_checkpoint(
    cp: &Checkpoint,
    sigs: &[[u8; 65]; 2],
    leg_a_session_key: &[u8; 20],
    leg_b_session_key: &[u8; 20],
) -> Result<[u8; 32], CosignError> {
    let digest = keccak256(&cp.encode());
    if recover_address(&digest, &sigs[0])? != *leg_a_session_key {
        return Err(CosignError::SignerMismatch);
    }
    if recover_address(&digest, &sigs[1])? != *leg_b_session_key {
        return Err(CosignError::SignerMismatch);
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(byte: u8) -> [u8; 32] {
        let mut s = [0u8; 32];
        s[31] = byte; // small but valid nonzero scalar
        s
    }

    #[test]
    fn signature_recovers_to_the_signer_address() {
        let sk = secret(0x42);
        let addr = eth_address(&sk).unwrap();
        let digest = keccak256(b"hello cross-chain");
        let sig = sign_digest(&sk, &digest).unwrap();

        // Recover exactly as the on-chain secp256k1_recover / EVM ecrecover do.
        let recid = RecoveryId::from_byte(sig[64]).unwrap();
        let signature = Signature::from_slice(&sig[..64]).unwrap();
        let recovered = VerifyingKey::recover_from_prehash(&digest, &signature, recid).unwrap();
        assert_eq!(verifying_key_address(&recovered), addr);
    }

    #[test]
    fn recover_address_rejects_high_s_malleable_signature() {
        let sk = secret(0x42);
        let digest = keccak256(b"malleable twin");
        let sig = sign_digest(&sk, &digest).unwrap();
        // The canonical low-s signature recovers fine.
        assert!(recover_address(&digest, &sig).is_ok());

        // Build the malleable high-s twin: s' = n - s (the recovery bit would
        // also flip, but the low-s guard fires before recovery).
        let signature = Signature::from_slice(&sig[..64]).unwrap();
        let neg_s = -(*signature.s());
        let twin = Signature::from_scalars(signature.r().to_bytes(), neg_s.to_bytes())
            .expect("valid scalars");
        let mut malleable = [0u8; 65];
        malleable[..64].copy_from_slice(&twin.to_bytes());
        malleable[64] = sig[64] ^ 1;
        assert_eq!(
            recover_address(&digest, &malleable),
            Err(CosignError::MalleableSignature)
        );
    }

    #[test]
    fn sign_match_live_digest_matches_canonical_encoding() {
        use crate::cert_schema::CertLeg;
        let leg = |s: u8| CertLeg {
            chain_tag: [s; 32],
            contract: [s.wrapping_add(1); 32],
            player: [s.wrapping_add(2); 32],
            session_key: [s.wrapping_add(3); 20],
            stake: u128::from(s),
            tranche: u128::from(s).wrapping_mul(2),
        };
        let cert = MatchLiveCert {
            match_id: [0xAA; 32],
            tournament_id: 7,
            matchup_commitment: [0xBB; 32],
            leg_a: leg(0x10),
            leg_b: leg(0x20),
            quote_timestamp: 1_765_000_000,
            quote_max_age_secs: 300,
            match_deadline: 1_765_000_900,
            claim_window_secs: 3600,
            a_is_p1: 1,
        };
        // The cosign path hashes the same bytes as the standalone digest.
        let sk = secret(0x99);
        let via_cert = sign_match_live(&sk, &cert).unwrap();
        let via_digest = sign_digest(&sk, &keccak256(&cert.encode())).unwrap();
        assert_eq!(via_cert, via_digest);
    }

    #[test]
    fn rejects_zero_secret_key() {
        assert_eq!(eth_address(&[0u8; 32]), Err(CosignError::BadSecretKey));
    }

    fn sample_checkpoint() -> Checkpoint {
        Checkpoint {
            match_live_digest: [0x11; 32],
            step_count: 4,
            p1_commit: [0x22; 32],
            p2_commit: [0x33; 32],
            p1_guess: 1,
            p2_guess: 0,
            first_committer: 1,
            matchup_type: 1,
            transcript_hash: [0x44; 32],
            r_matchup: [0x55; 32],
        }
    }

    #[test]
    fn verify_cosigned_checkpoint_accepts_both_session_keys() {
        let sk_a = secret(0x0a);
        let sk_b = secret(0x0b);
        let cp = sample_checkpoint();
        let sigs = [
            sign_checkpoint(&sk_a, &cp).unwrap(),
            sign_checkpoint(&sk_b, &cp).unwrap(),
        ];
        let digest = verify_cosigned_checkpoint(
            &cp,
            &sigs,
            &eth_address(&sk_a).unwrap(),
            &eth_address(&sk_b).unwrap(),
        )
        .expect("both signatures valid");
        assert_eq!(digest, keccak256(&cp.encode()));
    }

    #[test]
    fn verify_cosigned_checkpoint_rejects_swapped_signatures() {
        // sigs[0] must be leg A, sigs[1] leg B — order is part of the contract.
        let sk_a = secret(0x0a);
        let sk_b = secret(0x0b);
        let cp = sample_checkpoint();
        let swapped = [
            sign_checkpoint(&sk_b, &cp).unwrap(),
            sign_checkpoint(&sk_a, &cp).unwrap(),
        ];
        assert_eq!(
            verify_cosigned_checkpoint(
                &cp,
                &swapped,
                &eth_address(&sk_a).unwrap(),
                &eth_address(&sk_b).unwrap(),
            ),
            Err(CosignError::SignerMismatch)
        );
    }

    #[test]
    fn verify_cosigned_checkpoint_rejects_wrong_signer() {
        let sk_a = secret(0x0a);
        let sk_b = secret(0x0b);
        let imposter = secret(0x0c);
        let cp = sample_checkpoint();
        let sigs = [
            sign_checkpoint(&sk_a, &cp).unwrap(),
            sign_checkpoint(&imposter, &cp).unwrap(),
        ];
        assert_eq!(
            verify_cosigned_checkpoint(
                &cp,
                &sigs,
                &eth_address(&sk_a).unwrap(),
                &eth_address(&sk_b).unwrap(),
            ),
            Err(CosignError::SignerMismatch)
        );
    }

    #[test]
    fn verify_cosigned_checkpoint_rejects_signature_over_different_checkpoint() {
        // A signature over a DIFFERENT checkpoint recovers to some address but
        // not the expected session key → mismatch (tamper detection).
        let sk_a = secret(0x0a);
        let sk_b = secret(0x0b);
        let cp = sample_checkpoint();
        let mut tampered = cp.clone();
        tampered.step_count = 3;
        let sigs = [
            sign_checkpoint(&sk_a, &tampered).unwrap(),
            sign_checkpoint(&sk_b, &tampered).unwrap(),
        ];
        assert_eq!(
            verify_cosigned_checkpoint(
                &cp,
                &sigs,
                &eth_address(&sk_a).unwrap(),
                &eth_address(&sk_b).unwrap(),
            ),
            Err(CosignError::SignerMismatch)
        );
    }
}
