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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        match ns {
            "solana" | "eip155" => {}
            _ => return Err(CaipError::UnknownNamespace),
        }
        let len_ok = !reference.is_empty() && reference.len() <= 32;
        let chars_ok = reference
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !(len_ok && chars_ok) {
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

impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// A validated CAIP-10 account ID: `{caip2}:{address}`, e.g.
/// `eip155:84532:0xab16…` or `solana:EtWTRABZ…:CKsZf8gZ…`.
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

impl AccountId {
    /// Parse and validate a CAIP-10 string.
    pub fn parse(input: &str) -> Result<Self, CaipError> {
        let (chain_part, address) = input.rsplit_once(':').ok_or(CaipError::MissingSeparator)?;
        ChainId::parse(chain_part)?;
        let len_ok = !address.is_empty() && address.len() <= 128;
        let chars_ok = address
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'%'));
        if !(len_ok && chars_ok) {
            return Err(CaipError::BadAccountAddress);
        }
        let address_at = chain_part.len().saturating_add(1);
        Ok(AccountId {
            raw: input.to_owned(),
            address_at,
        })
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

    /// The bare address part (namespace-specific format).
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
    fn chain_id_rejects_invalid() {
        assert_eq!(
            ChainId::parse("cosmos:hub-4"),
            Err(CaipError::UnknownNamespace)
        );
        assert_eq!(ChainId::parse("eip155"), Err(CaipError::MissingSeparator));
        assert_eq!(ChainId::parse("eip155:"), Err(CaipError::BadReference));
        assert_eq!(
            ChainId::parse("eip155:has space"),
            Err(CaipError::BadReference)
        );
        let too_long = format!("eip155:{}", "9".repeat(33));
        assert_eq!(ChainId::parse(&too_long), Err(CaipError::BadReference));
    }

    #[test]
    fn account_id_round_trips() {
        let acct =
            AccountId::parse("eip155:84532:0x996213ed4099707059b8b5d7489ffF23dAC9770d").unwrap();
        assert_eq!(acct.chain().as_str(), "eip155:84532");
        assert_eq!(acct.address(), "0x996213ed4099707059b8b5d7489ffF23dAC9770d");

        let chain = ChainId::parse("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1").unwrap();
        let built =
            AccountId::from_parts(&chain, "CKsZf8gZzfPdEXHxLUEPvAi5C5sXBV9aDhqmsM7yTipv").unwrap();
        assert_eq!(built.chain(), chain);
    }

    #[test]
    fn account_id_rejects_invalid() {
        assert_eq!(
            AccountId::parse("eip155:84532:"),
            Err(CaipError::BadAccountAddress)
        );
        assert_eq!(
            AccountId::parse("cosmos:hub-4:addr"),
            Err(CaipError::UnknownNamespace)
        );
    }
}
