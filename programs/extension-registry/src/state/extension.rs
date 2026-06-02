use anchor_lang::prelude::*;

/// An active extension obligation. Exists only while the obligation is
/// outstanding; closed on `attest_return_substance` or `default_extension`.
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
    /// The agent receiving the extension (owes return-substance).
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
    /// Reserved for future fields without reallocation.
    pub _reserved: [u8; 16],
}

impl Extension {
    // 8 + 32 + 32 + 1 + 8 + 8 + 1 + 16 = 106
    pub const SPACE: usize = 8   // discriminator
        + 32   // extender
        + 32   // recipient
        + 1    // extension_type
        + 8    // bond_lamports
        + 8    // created_at
        + 1    // bump
        + 16; // _reserved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_space_is_106() {
        assert_eq!(Extension::SPACE, 106);
    }
}
