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
}
