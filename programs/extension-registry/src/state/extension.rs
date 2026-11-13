use anchor_lang::prelude::*;

/// A live vouch edge. Exists only while the extender is actively backing the
/// recipient; closed on `withdraw_extension` (extender reclaims it) or
/// `default_extension` (authority slashes a default).
/// Holds the extender's bond (rent-exempt minimum + `bond_lamports`).
///
/// Seeds: `["extension", extender, recipient]` — one active extension per
/// (extender, recipient) pair. `init` (not `init_if_needed`) prevents
/// resurrection while active; after a terminal transition closes it, the pair
/// can extend again. The durable history is the emitted event stream, so a
/// closed account loses nothing the indexer needs.
#[account]
pub struct Extension {
    /// The agent extending standing (locks the bond).
    pub extender: Pubkey,
    /// The agent receiving the vouch (holds borrowed standing while it's live).
    pub recipient: Pubkey,
    /// Extension type discriminant (see `constants`). MVP: CapabilityValidation.
    pub extension_type: u8,
    /// Bond locked by the extender, in lamports (held on this account, on top
    /// of the rent-exempt minimum).
    pub bond_lamports: u64,
    /// Unix timestamp the extension was created (time-decay input off-chain).
    pub created_at: i64,
    /// PDA bump.
    pub bump: u8,
    /// Unix timestamp when `request_unbond` was called (`0` while actively
    /// bonded; `> 0` once the 7-day unbonding cooldown starts). Carved from
    /// `_reserved` so live mainnet `Extension` accounts (`_reserved = [0u8; 16]`)
    /// deserialize seamlessly as `unbond_requested_at == 0` with `SPACE == 106`.
    pub unbond_requested_at: i64,
    /// Reserved for future fields without reallocation.
    pub _reserved: [u8; 8],
}

impl Extension {
    // 8 + 32 + 32 + 1 + 8 + 8 + 1 + 8 + 8 = 106
    pub const SPACE: usize = 8   // discriminator
        + 32   // extender
        + 32   // recipient
        + 1    // extension_type
        + 8    // bond_lamports
        + 8    // created_at
        + 1    // bump
        + 8    // unbond_requested_at
        + 8; // _reserved

    /// True once `request_unbond` has been invoked on this extension.
    pub fn is_unbonding(&self) -> bool {
        self.unbond_requested_at > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_space_is_106_and_legacy_zero_reserved_deserializes_as_active() {
        assert_eq!(Extension::SPACE, 106);

        let extender = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let mut legacy_bytes = Vec::with_capacity(Extension::SPACE);
        legacy_bytes.extend_from_slice(Extension::DISCRIMINATOR);
        legacy_bytes.extend_from_slice(extender.as_ref());
        legacy_bytes.extend_from_slice(recipient.as_ref());
        legacy_bytes.push(0u8); // extension_type
        legacy_bytes.extend_from_slice(&5_000_000u64.to_le_bytes());
        legacy_bytes.extend_from_slice(&1_700_000_000i64.to_le_bytes());
        legacy_bytes.push(254u8); // bump
        legacy_bytes.extend_from_slice(&[0u8; 16]); // legacy _reserved = [0u8; 16]
        assert_eq!(legacy_bytes.len(), Extension::SPACE);

        let decoded = Extension::try_deserialize(&mut legacy_bytes.as_slice()).unwrap();
        assert_eq!(decoded.extender, extender);
        assert_eq!(decoded.recipient, recipient);
        assert_eq!(decoded.bond_lamports, 5_000_000);
        assert_eq!(decoded.unbond_requested_at, 0);
        assert!(!decoded.is_unbonding());
    }
}
