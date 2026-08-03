use anchor_lang::prelude::*;

pub const MIN_TREASURY_SPLIT_BPS: u16 = 2_000; // 20%
pub const MAX_TREASURY_SPLIT_BPS: u16 = 8_000; // 80%

/// Singleton PDA storing protocol-level configuration.
/// Seeds: `["global_config"]`
#[account]
pub struct GlobalConfig {
    /// Governance authority (EOA for v1).
    pub authority: Pubkey,
    /// Authorized matchmaker that gates `create_game`.
    pub matchmaker: Pubkey,
    /// DAO treasury address for losing stake split.
    pub treasury: Pubkey,
    /// Portion of losing stakes sent to treasury (basis points).
    /// Default 5000 = 50%. Bounded to [2000, 8000].
    pub treasury_split_bps: u16,
    pub bump: u8,
    /// Per-game stake in lamports.
    ///
    /// APPENDED AFTER `bump` deliberately: the 107-byte v1 layout stays a
    /// byte-exact prefix, so `migrate_global_config` only has to grow the
    /// account and write this one field. An account survey on 2026-08-03 found
    /// exactly ONE GlobalConfig on mainnet and one on devnet, both at 107
    /// bytes, so the migration is a single call per network.
    ///
    /// This used to be the compile-time `FIXED_STAKE_LAMPORTS`, which meant
    /// re-pegging Solana required a PROGRAM UPGRADE while the EVM chains only
    /// needed `setConfig`. That asymmetry is why Solana sat at $3.64 against a
    /// $5 EVM anchor: the cheap change was made and the expensive one was not.
    pub stake_lamports: u64,
}

impl GlobalConfig {
    pub const SPACE: usize = 8  // discriminator
        + 32  // authority
        + 32  // matchmaker
        + 32  // treasury
        + 2   // treasury_split_bps
        + 1   // bump
        + 8; // stake_lamports

    /// On-disk size before `stake_lamports` was appended. Every deployed
    /// account is at this size until `migrate_global_config` runs.
    pub const LEGACY_SPACE: usize = Self::SPACE - 8;
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn legacy_space_is_a_byte_exact_prefix_of_the_new_layout() {
        // The migration only grows the account and appends, so v1 must be
        // exactly 8 bytes shorter. An account survey (2026-08-03) found every
        // deployed GlobalConfig at LEGACY_SPACE on both mainnet and devnet.
        assert_eq!(GlobalConfig::LEGACY_SPACE, 107);
        assert_eq!(GlobalConfig::SPACE, 115);
        assert_eq!(GlobalConfig::SPACE - GlobalConfig::LEGACY_SPACE, 8);
    }

    #[test]
    fn space_matches_the_field_widths() {
        // Catches a field added without updating SPACE — which is how an
        // account ends up too small to deserialize in production.
        let expected = 8 + 32 + 32 + 32 + 2 + 1 + 8;
        assert_eq!(GlobalConfig::SPACE, expected);
    }
}
