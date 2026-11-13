//! Cryptographic & claim-level separation between end-user JWTs (`typ: "user"`)
//! and internal service-to-service tokens (`typ: "svc"`).
//!
//! Mirrors `module_transport::wallet` in `coordination-app`:
//! - End-user tokens are verified with the raw `jwt_secret` (`HS256`) and must
//!   carry `typ == "user"` + matching `aud`.
//! - Internal service tokens (`mint_svc_token`) are signed with a domain-separated
//!   HMAC-SHA256 subkey (`derive_svc_key`, message `b"swarm:jwt:svc:v1"`), carry
//!   `typ: "svc"` and `aud: ["game-internal"]`, have a 60s TTL, and require a
//!   [`crate::session_binding::VerifiedWallet`] at compile time.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Lifetime of a minted internal service token (seconds).
pub const INTERNAL_TOKEN_TTL_SECS: u64 = 60;

const SVC_KEY_DOMAIN: &[u8] = b"swarm:jwt:svc:v1";

/// Normalize a wallet identifier via [`chain_core::CanonicalWallet`]: lowercases
/// `0x`-prefixed EVM addresses and `eip155:` CAIP-10 account segments while
/// preserving exact case for Solana base58 pubkeys.
pub fn normalize_wallet(wallet: &str) -> String {
    let trimmed = wallet.trim();
    if let Ok(canon) = chain_core::CanonicalWallet::parse(trimmed) {
        return canon.as_str().to_owned();
    }
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        return trimmed.to_ascii_lowercase();
    }
    if trimmed.starts_with("eip155:") {
        if let Some(idx) = trimmed.rfind(':') {
            let (prefix, addr) = trimmed.split_at(idx);
            return format!("{prefix}{}", addr.to_ascii_lowercase());
        }
    }
    trimmed.to_owned()
}

/// Compare two wallet strings in canonical form: case-insensitive for bare `0x`
/// and `eip155:` CAIP-10 EVM addresses, exact case-sensitive match for Solana base58.
pub fn wallets_match(a: &str, b: &str) -> bool {
    if let (Ok(ca), Ok(cb)) = (
        chain_core::CanonicalWallet::parse(a),
        chain_core::CanonicalWallet::parse(b),
    ) {
        return ca == cb;
    }
    normalize_wallet(a) == normalize_wallet(b)
}

/// Derive the internal service-token signing key from `jwt_secret` via
/// `HMAC-SHA256(key = jwt_secret.as_bytes(), msg = b"swarm:jwt:svc:v1")`.
/// Returns `None` when `jwt_secret` is empty.
pub fn derive_svc_key(jwt_secret: &str) -> Option<[u8; 32]> {
    if jwt_secret.is_empty() {
        return None;
    }
    let mut mac = HmacSha256::new_from_slice(jwt_secret.as_bytes()).ok()?;
    mac.update(SVC_KEY_DOMAIN);
    Some(mac.finalize().into_bytes().into())
}

/// Mint a 60s internal service token (`typ: "svc"`, `aud: ["game-internal"]`)
/// signed with [`derive_svc_key`]. Accepts only [`crate::session_binding::VerifiedWallet`]
/// so an unverified wallet string can never mint a service token at compile time.
pub fn mint_svc_token(
    wallet: &crate::session_binding::VerifiedWallet,
    jwt_secret: &str,
) -> Option<String> {
    if wallet.as_str().is_empty() || jwt_secret.is_empty() {
        return None;
    }
    let key = derive_svc_key(jwt_secret)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let exp = now.checked_add(INTERNAL_TOKEN_TTL_SECS)?;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let claims = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&serde_json::json!({
            "wallet": normalize_wallet(wallet.as_str()),
            "exp": exp,
            "typ": "svc",
            "aud": ["game-internal"],
        }))
        .ok()?,
    );
    let signing_input = format!("{header}.{claims}");
    let mut mac = HmacSha256::new_from_slice(&key).ok()?;
    mac.update(signing_input.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Some(format!("{signing_input}.{sig}"))
}

/// Verify an end-user HS256 JWT signed with raw `jwt_secret.as_bytes()`,
/// enforcing `exp > now` (0 leeway), non-empty `wallet`, `typ == "user"`, and
/// `expected_aud` in `aud` (`MED-10`: legacy `typ: None` tokens are retired).
pub fn verify_user_jwt(
    token: &str,
    jwt_secret: &str,
    expected_aud: &str,
) -> Result<String, &'static str> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "clock_error")?
        .as_secs();
    verify_user_jwt_at(token, jwt_secret, expected_aud, now)
}

pub fn verify_user_jwt_at(
    token: &str,
    jwt_secret: &str,
    expected_aud: &str,
    now: u64,
) -> Result<String, &'static str> {
    if jwt_secret.is_empty() {
        return Err("missing_secret");
    }
    if expected_aud.is_empty() {
        return Err("missing_expected_audience");
    }
    let mut parts = token.trim().split('.');
    let (Some(header_b64), Some(claims_b64), Some(sig_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("malformed_token");
    };
    if header_b64.is_empty() || claims_b64.is_empty() || sig_b64.is_empty() {
        return Err("malformed_token");
    }

    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|_| "invalid_header_encoding")?;
    let header_json: serde_json::Value =
        serde_json::from_slice(&header_bytes).map_err(|_| "invalid_header_json")?;
    if header_json.get("alg").and_then(serde_json::Value::as_str) != Some("HS256") {
        return Err("unsupported_alg");
    }

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| "invalid_signature_encoding")?;
    let mut mac =
        HmacSha256::new_from_slice(jwt_secret.as_bytes()).map_err(|_| "invalid_secret")?;
    mac.update(header_b64.as_bytes());
    mac.update(b".");
    mac.update(claims_b64.as_bytes());
    mac.verify_slice(&sig_bytes)
        .map_err(|_| "invalid_signature")?;

    let claims_bytes = URL_SAFE_NO_PAD
        .decode(claims_b64)
        .map_err(|_| "invalid_claims_encoding")?;
    let claims: serde_json::Value =
        serde_json::from_slice(&claims_bytes).map_err(|_| "invalid_claims_json")?;
    let obj = claims.as_object().ok_or("claims_not_object")?;

    let exp = obj
        .get("exp")
        .and_then(serde_json::Value::as_u64)
        .ok_or("missing_exp")?;
    if exp <= now {
        return Err("token_expired");
    }

    let wallet = obj
        .get("wallet")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .ok_or("missing_wallet")?;

    match obj.get("typ") {
        Some(serde_json::Value::String(typ)) if typ == "user" => {
            if !aud_contains(obj.get("aud"), expected_aud) {
                return Err("invalid_audience");
            }
            Ok(normalize_wallet(wallet))
        }
        None | Some(serde_json::Value::Null) => Err("missing_token_type"),
        Some(_) => Err("invalid_token_type"),
    }
}

fn aud_contains(aud: Option<&serde_json::Value>, expected: &str) -> bool {
    match aud {
        Some(serde_json::Value::String(single)) => single == expected,
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .any(|item| item.as_str().is_some_and(|s| s == expected)),
        _ => false,
    }
}

#[cfg(test)]
pub(crate) fn sign_hs256_for_test(claims: &serde_json::Value, key: &[u8]) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).expect("claims json"));
    let signing_input = format!("{header}.{payload}");
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
    mac.update(signing_input.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{signing_input}.{sig}")
}

#[cfg(test)]
pub(crate) fn mint_user_jwt_for_test(wallet: &str, secret: &str, aud: &[&str], exp: u64) -> String {
    sign_hs256_for_test(
        &serde_json::json!({
            "wallet": wallet,
            "exp": exp,
            "typ": "user",
            "aud": aud,
        }),
        secret.as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_binding::VerifiedWallet;
    use serde_json::json;

    const SECRET: &str = "test-jwt-secret-key-32-bytes-min";
    const SOL_WALLET: &str = "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";
    const EVM_MIXED: &str = "0x996213ed4099707059B8B5D7489FFF23DAC9770D";
    const EVM_LOWER: &str = "0x996213ed4099707059b8b5d7489fff23dac9770d";

    fn future_exp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
            .saturating_add(300)
    }

    #[test]
    fn derive_svc_key_is_deterministic_domain_separated_and_rejects_empty() {
        assert!(derive_svc_key("").is_none());
        let k1 = derive_svc_key(SECRET).expect("key");
        let k2 = derive_svc_key(SECRET).expect("key");
        let k_other = derive_svc_key("different-secret").expect("key");
        assert_eq!(k1, k2);
        assert_ne!(k1, k_other);
        assert_ne!(k1.as_slice(), SECRET.as_bytes());
    }

    #[test]
    fn mint_svc_token_uses_derived_key_60s_ttl_and_svc_claims() {
        let verified = VerifiedWallet::new_for_test(EVM_MIXED);
        let token = mint_svc_token(&verified, SECRET).expect("mints svc token");
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);

        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["wallet"], EVM_LOWER);
        assert_eq!(claims["typ"], "svc");
        assert_eq!(claims["aud"], json!(["game-internal"]));

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let exp = claims["exp"].as_u64().unwrap();
        assert!(exp > now && exp <= now.saturating_add(INTERNAL_TOKEN_TTL_SECS));

        // Verifies with derived key, NOT raw secret.
        let derived = derive_svc_key(SECRET).unwrap();
        let mut mac_ok = HmacSha256::new_from_slice(&derived).unwrap();
        mac_ok.update(format!("{}.{}", parts[0], parts[1]).as_bytes());
        mac_ok
            .verify_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap())
            .expect("derived key verifies");

        let mut mac_raw = HmacSha256::new_from_slice(SECRET.as_bytes()).unwrap();
        mac_raw.update(format!("{}.{}", parts[0], parts[1]).as_bytes());
        assert!(
            mac_raw
                .verify_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap())
                .is_err(),
            "raw user secret must NOT verify svc token"
        );
    }

    #[test]
    fn c7_svc_token_is_rejected_by_verify_user_jwt_even_if_forged_with_raw_secret() {
        let verified = VerifiedWallet::new_for_test(SOL_WALLET);
        let svc_token = mint_svc_token(&verified, SECRET).expect("mints");
        assert_eq!(
            verify_user_jwt(&svc_token, SECRET, "mcp"),
            Err("invalid_signature")
        );

        let raw_signed_svc = sign_hs256_for_test(
            &json!({
                "wallet": SOL_WALLET,
                "exp": future_exp(),
                "typ": "svc",
                "aud": ["mcp", "game-internal"],
            }),
            SECRET.as_bytes(),
        );
        assert_eq!(
            verify_user_jwt(&raw_signed_svc, SECRET, "mcp"),
            Err("invalid_token_type")
        );
    }

    #[test]
    fn verify_user_jwt_accepts_valid_user_token_and_normalizes_evm() {
        let sol_tok = mint_user_jwt_for_test(
            SOL_WALLET,
            SECRET,
            &["shillbot", "game", "mcp"],
            future_exp(),
        );
        assert_eq!(
            verify_user_jwt(&sol_tok, SECRET, "mcp").expect("valid sol user jwt"),
            SOL_WALLET
        );

        let evm_tok = sign_hs256_for_test(
            &json!({
                "wallet": EVM_MIXED,
                "exp": future_exp(),
                "typ": "user",
                "aud": "mcp",
            }),
            SECRET.as_bytes(),
        );
        assert_eq!(
            verify_user_jwt(&evm_tok, SECRET, "mcp").expect("valid string-aud evm jwt"),
            EVM_LOWER
        );
    }

    #[test]
    fn verify_user_jwt_enforces_audience_expiry_and_rejects_missing_typ() {
        let now = 1_794_400_000_u64;
        let wrong_aud = mint_user_jwt_for_test(
            SOL_WALLET,
            SECRET,
            &["shillbot", "game"],
            now.saturating_add(300),
        );
        assert_eq!(
            verify_user_jwt_at(&wrong_aud, SECRET, "mcp", now),
            Err("invalid_audience")
        );

        let expired = mint_user_jwt_for_test(SOL_WALLET, SECRET, &["mcp"], now);
        assert_eq!(
            verify_user_jwt_at(&expired, SECRET, "mcp", now),
            Err("token_expired")
        );

        // Legacy typ:None tokens are unconditionally rejected (`MED-10`).
        let legacy_typ_none = sign_hs256_for_test(
            &json!({
                "wallet": SOL_WALLET,
                "exp": now.saturating_add(300),
                "aud": ["mcp"],
            }),
            SECRET.as_bytes(),
        );
        assert_eq!(
            verify_user_jwt_at(&legacy_typ_none, SECRET, "mcp", now),
            Err("missing_token_type")
        );
    }

    #[test]
    fn wallets_match_is_case_insensitive_for_evm_and_caip10_and_exact_for_solana() {
        assert!(wallets_match(EVM_MIXED, EVM_LOWER));
        assert!(wallets_match(
            &format!("eip155:8453:{EVM_MIXED}"),
            &format!("eip155:8453:{EVM_LOWER}")
        ));
        assert!(!wallets_match(EVM_LOWER, SOL_WALLET));
        assert!(wallets_match(SOL_WALLET, SOL_WALLET));
        let mutated_sol = "cksZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY";
        assert!(!wallets_match(SOL_WALLET, mutated_sol));
    }
}
