//! CAIP-2 chain identifiers and CAIP-10 account identifiers.
//!
//! Every off-chain data structure that names a chain uses these types
//! (API fields, Firestore docs, MCP params, attestations, events).
//! On-chain account layouts are exempt — `Pubkey`/`address` stay native
//! and chain context attaches at the serialization boundary.

use core::fmt;

/// The chain namespaces this org supports. Services dispatch on
/// namespace, never on individual chains — adding a chain within a
/// namespace is a chain-registry config entry, not a code change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Namespace {
    Solana,
    Eip155,
}

impl Namespace {
    pub fn as_str(&self) -> &'static str {
        match self {
            Namespace::Solana => "solana",
            Namespace::Eip155 => "eip155",
        }
    }
}

/// A validated CAIP-2 chain ID, e.g. `solana:EtWTRABZ…` or `eip155:84532`.
///
/// Construction validates shape AND namespace: unknown namespaces are
/// rejected at the boundary rather than carried as opaque strings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize), serde(transparent))]
pub struct ChainId {
    raw: String,
}

/// Why a CAIP identifier failed to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaipError {
    MissingSeparator,
    UnknownNamespace,
    BadReference,
    BadAccountAddress,
}

impl fmt::Display for CaipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            CaipError::MissingSeparator => "missing ':' separator",
            CaipError::UnknownNamespace => "unknown namespace (expected solana or eip155)",
            CaipError::BadReference => "invalid chain reference (1-32 chars of [-_a-zA-Z0-9])",
            CaipError::BadAccountAddress => {
                "invalid account address (1-128 chars of [-.%a-zA-Z0-9])"
            }
        };
        f.write_str(msg)
    }
}

impl ChainId {
    /// Parse and validate a CAIP-2 string.
    pub fn parse(input: &str) -> Result<Self, CaipError> {
        let (ns, reference) = input.split_once(':').ok_or(CaipError::MissingSeparator)?;
        let valid_ref = match ns {
            "eip155" => {
                // EIP-155 chain IDs are positive decimal integers with no leading zero.
                !reference.is_empty()
                    && reference.len() <= 20
                    && !reference.starts_with('0')
                    && reference.bytes().all(|b| b.is_ascii_digit())
                    && reference.parse::<u64>().map(|v| v > 0).unwrap_or(false)
            }
            "solana" => {
                // Solana CAIP-2 references are 1..=32 base58 chars (genesis hash prefix).
                const B58: &[u8; 58] =
                    b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
                !reference.is_empty()
                    && reference.len() <= 32
                    && reference.bytes().all(|b| B58.contains(&b))
            }
            _ => return Err(CaipError::UnknownNamespace),
        };
        if !valid_ref {
            return Err(CaipError::BadReference);
        }
        Ok(ChainId {
            raw: input.to_owned(),
        })
    }

    pub fn namespace(&self) -> Namespace {
        // Validated at construction; the prefix is one of the two.
        if self.raw.starts_with("solana:") {
            Namespace::Solana
        } else {
            Namespace::Eip155
        }
    }

    /// The chain reference (the part after `:`), e.g. `84532`.
    pub fn reference(&self) -> &str {
        // Validated at construction; the separator exists.
        self.raw.split_once(':').map(|(_, r)| r).unwrap_or("")
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ChainId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// A validated, canonicalized CAIP-10 account ID: `{caip2}:{address}`, e.g.
/// `eip155:84532:0xab16…` (EVM hex normalized to lowercase) or
/// `solana:EtWTRABZ…:CKsZf8gZ…` (32-byte Ed25519 base58 case preserved).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountId {
    raw: String,
    address_at: usize,
}

#[cfg(feature = "serde")]
impl serde::Serialize for AccountId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.raw)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for AccountId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl AccountId {
    /// Parse, validate, and canonicalize a CAIP-10 string.
    ///
    /// - `eip155`: validates `0x` + 40 hex chars and normalizes the address to lowercase.
    /// - `solana`: validates exact 32-byte base58 public key and preserves exact case.
    pub fn parse(input: &str) -> Result<Self, CaipError> {
        let (chain_part, address) = input.rsplit_once(':').ok_or(CaipError::MissingSeparator)?;
        let chain = ChainId::parse(chain_part)?;
        let canonical_addr = match chain.namespace() {
            Namespace::Eip155 => {
                if crate::onchain_action::crypto::decode_0x_hex_exact::<20>(address).is_none() {
                    return Err(CaipError::BadAccountAddress);
                }
                address.to_ascii_lowercase()
            }
            Namespace::Solana => {
                if crate::onchain_action::crypto::decode_base58_exact::<32>(address).is_none() {
                    return Err(CaipError::BadAccountAddress);
                }
                address.to_owned()
            }
        };
        let address_at = chain_part.len().saturating_add(1);
        let mut raw = String::with_capacity(address_at.saturating_add(canonical_addr.len()));
        raw.push_str(chain_part);
        raw.push(':');
        raw.push_str(&canonical_addr);
        Ok(AccountId { raw, address_at })
    }

    /// Build from a validated chain ID and a raw address string.
    pub fn from_parts(chain: &ChainId, address: &str) -> Result<Self, CaipError> {
        let mut raw = String::with_capacity(
            chain
                .as_str()
                .len()
                .saturating_add(1)
                .saturating_add(address.len()),
        );
        raw.push_str(chain.as_str());
        raw.push(':');
        raw.push_str(address);
        Self::parse(&raw)
    }

    pub fn chain(&self) -> ChainId {
        // Validated at construction.
        ChainId {
            raw: self.raw[..self.address_at.saturating_sub(1)].to_owned(),
        }
    }

    /// The bare canonical address part (`0x` + lowercase 40-hex or base58).
    pub fn address(&self) -> &str {
        &self.raw[self.address_at..]
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// Canonical wallet identity across bare addresses (`0x...` / Solana base58)
/// and CAIP-10 account IDs (`eip155:<id>:0x...` / `solana:<ref>:<b58>`).
///
/// Enforces parse-don't-validate invariants at trust boundaries:
/// - EVM addresses (`0x...` or `eip155:<id>:0x...`) are validated as 20-byte
///   hex and normalized to **lowercase**, eliminating EIP-55 vs lowercase
///   key-lookup, session-binding, and self-match bypasses.
/// - Solana pubkeys (`<b58>` or `solana:<ref>:<b58>`) are validated as exact
///   32-byte Ed25519 base58 and preserve exact case.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanonicalWallet {
    raw: String,
    namespace: Namespace,
    bare_offset: usize,
}

#[cfg(feature = "serde")]
impl serde::Serialize for CanonicalWallet {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.raw)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for CanonicalWallet {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl CanonicalWallet {
    /// Parse and canonicalize a bare wallet address or CAIP-10 `AccountId`.
    pub fn parse(input: &str) -> Result<Self, CaipError> {
        let trimmed = input.trim();
        if trimmed.contains(':') {
            let acct = AccountId::parse(trimmed)?;
            let namespace = acct.chain().namespace();
            let bare_offset = acct.address_at;
            return Ok(Self {
                raw: acct.raw,
                namespace,
                bare_offset,
            });
        }
        if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
            let normalized = if let Some(rest) = trimmed.strip_prefix("0X") {
                let mut s = String::with_capacity(trimmed.len());
                s.push_str("0x");
                s.push_str(rest);
                s
            } else {
                trimmed.to_owned()
            };
            if crate::onchain_action::crypto::decode_0x_hex_exact::<20>(&normalized).is_none() {
                return Err(CaipError::BadAccountAddress);
            }
            return Ok(Self {
                raw: normalized.to_ascii_lowercase(),
                namespace: Namespace::Eip155,
                bare_offset: 0,
            });
        }
        if crate::onchain_action::crypto::decode_base58_exact::<32>(trimmed).is_none() {
            return Err(CaipError::BadAccountAddress);
        }
        Ok(Self {
            raw: trimmed.to_owned(),
            namespace: Namespace::Solana,
            bare_offset: 0,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn into_string(self) -> String {
        self.raw
    }

    pub fn namespace(&self) -> Namespace {
        self.namespace
    }

    pub fn is_evm(&self) -> bool {
        self.namespace == Namespace::Eip155
    }

    pub fn is_solana(&self) -> bool {
        self.namespace == Namespace::Solana
    }

    /// Bare canonical address (`0x` + 40 lowercase hex chars, or 32-byte base58),
    /// stripping any `eip155:<id>:` / `solana:<ref>:` prefix.
    pub fn bare_address(&self) -> &str {
        &self.raw[self.bare_offset..]
    }

    /// True if both wallets belong to the same chain family and resolve to the
    /// identical underlying cryptographic address (even if one is bare `0xabcd`
    /// and the other is `eip155:8453:0xAbCd`).
    pub fn same_underlying_address(&self, other: &Self) -> bool {
        self.namespace == other.namespace && self.bare_address() == other.bare_address()
    }
}

impl fmt::Display for CanonicalWallet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// Validation failure for [`SafePathSegment`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegmentError {
    Empty,
    TooLong,
    DotSegment,
    ForbiddenChar,
}

impl fmt::Display for PathSegmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::Empty => "path segment must not be empty",
            Self::TooLong => "path segment exceeds 128 bytes",
            Self::DotSegment => "relative dot path segments are forbidden",
            Self::ForbiddenChar => "path segment contains forbidden characters",
        };
        f.write_str(msg)
    }
}

/// URL path-segment wrapper preventing path traversal (`..`, `/`, `\`, `%2f`,
/// `?`, `#`, control bytes) when interpolating resource IDs or wallets into
/// upstream HTTP URLs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SafePathSegment {
    encoded: String,
}

impl SafePathSegment {
    /// Validate `raw` (`1..=128` bytes of `[A-Za-z0-9:._-]`, excluding `.` and `..`)
    /// as a single unambiguous RFC 3986 `pchar` path segment (`:` is legal in
    /// non-initial URI path segments and required verbatim by `camp:idx` task IDs).
    pub fn parse(raw: &str) -> Result<Self, PathSegmentError> {
        if raw.is_empty() {
            return Err(PathSegmentError::Empty);
        }
        if raw.len() > 128 {
            return Err(PathSegmentError::TooLong);
        }
        if raw == "." || raw == ".." {
            return Err(PathSegmentError::DotSegment);
        }
        for b in raw.bytes() {
            if !matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b':') {
                return Err(PathSegmentError::ForbiddenChar);
            }
        }
        Ok(Self {
            encoded: raw.to_owned(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.encoded
    }
}

impl fmt::Display for SafePathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_id_accepts_known_namespaces() {
        let sol = ChainId::parse("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1").unwrap();
        assert_eq!(sol.namespace(), Namespace::Solana);
        assert_eq!(sol.reference(), "EtWTRABZaYq6iMfeYKouRu166VU2xqa1");

        let base = ChainId::parse("eip155:84532").unwrap();
        assert_eq!(base.namespace(), Namespace::Eip155);
        assert_eq!(base.reference(), "84532");
    }

    #[test]
    fn chain_id_rejects_invalid_and_leading_zeros() {
        assert_eq!(
            ChainId::parse("cosmos:hub-4"),
            Err(CaipError::UnknownNamespace)
        );
        assert_eq!(ChainId::parse("eip155"), Err(CaipError::MissingSeparator));
        assert_eq!(ChainId::parse("eip155:"), Err(CaipError::BadReference));
        assert_eq!(ChainId::parse("eip155:0"), Err(CaipError::BadReference));
        assert_eq!(ChainId::parse("eip155:056"), Err(CaipError::BadReference));
        assert_eq!(ChainId::parse("eip155:abc"), Err(CaipError::BadReference));
        assert_eq!(
            ChainId::parse("eip155:has space"),
            Err(CaipError::BadReference)
        );
        assert_eq!(
            ChainId::parse("solana:invalid_0OIl"),
            Err(CaipError::BadReference)
        );
        let too_long = format!("eip155:{}", "9".repeat(33));
        assert_eq!(ChainId::parse(&too_long), Err(CaipError::BadReference));
    }

    #[test]
    fn account_id_normalizes_evm_lowercase_and_preserves_solana_base58() {
        let acct =
            AccountId::parse("eip155:84532:0x996213ed4099707059b8b5d7489ffF23dAC9770d").unwrap();
        assert_eq!(acct.chain().as_str(), "eip155:84532");
        assert_eq!(acct.address(), "0x996213ed4099707059b8b5d7489fff23dac9770d");
        assert_eq!(
            acct.as_str(),
            "eip155:84532:0x996213ed4099707059b8b5d7489fff23dac9770d"
        );

        let chain = ChainId::parse("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1").unwrap();
        let built =
            AccountId::from_parts(&chain, "CKsZf8gZzfPdEXHxLUEPvAi5C5sXBV9aDhqmsM7yTipv").unwrap();
        assert_eq!(built.chain(), chain);
        assert_eq!(
            built.address(),
            "CKsZf8gZzfPdEXHxLUEPvAi5C5sXBV9aDhqmsM7yTipv"
        );
    }

    #[test]
    fn account_id_rejects_invalid() {
        assert_eq!(
            AccountId::parse("eip155:84532:"),
            Err(CaipError::BadAccountAddress)
        );
        assert_eq!(
            AccountId::parse("eip155:84532:0x1234"),
            Err(CaipError::BadAccountAddress)
        );
        assert_eq!(
            AccountId::parse("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1:not32bytes"),
            Err(CaipError::BadAccountAddress)
        );
        assert_eq!(
            AccountId::parse("cosmos:hub-4:addr"),
            Err(CaipError::UnknownNamespace)
        );
    }

    #[test]
    fn canonical_wallet_unifies_mixed_case_and_caip10() {
        let checksum =
            CanonicalWallet::parse("0x996213ed4099707059b8b5d7489ffF23dAC9770d").unwrap();
        let lower = CanonicalWallet::parse("0x996213ed4099707059b8b5d7489fff23dac9770d").unwrap();
        let caip = CanonicalWallet::parse("eip155:8453:0x996213ed4099707059b8b5d7489ffF23dAC9770d")
            .unwrap();
        assert_eq!(checksum, lower);
        assert!(checksum.same_underlying_address(&caip));
        assert_eq!(
            caip.as_str(),
            "eip155:8453:0x996213ed4099707059b8b5d7489fff23dac9770d"
        );

        let sol = CanonicalWallet::parse("CKsZf8gZzfPdEXHxLUEPvAi5C5sXBV9aDhqmsM7yTipv").unwrap();
        assert!(sol.is_solana());
        assert!(!sol.same_underlying_address(&checksum));
    }

    #[test]
    fn safe_path_segment_blocks_traversal_and_preserves_colons() {
        assert_eq!(
            SafePathSegment::parse(".."),
            Err(PathSegmentError::DotSegment)
        );
        assert_eq!(
            SafePathSegment::parse("../internal/admin"),
            Err(PathSegmentError::ForbiddenChar)
        );
        assert_eq!(
            SafePathSegment::parse("foo%2fbar"),
            Err(PathSegmentError::ForbiddenChar)
        );
        assert_eq!(
            SafePathSegment::parse("id?admin=1"),
            Err(PathSegmentError::ForbiddenChar)
        );
        let seg = SafePathSegment::parse("eip155:8453:0x996213ed4099707059b8b5d7489fff23dac9770d")
            .unwrap();
        assert_eq!(
            seg.as_str(),
            "eip155:8453:0x996213ed4099707059b8b5d7489fff23dac9770d"
        );
    }
}
