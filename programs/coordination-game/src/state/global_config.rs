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
}

impl GlobalConfig {
    pub const SPACE: usize = 8  // discriminator
        + 32  // authority
        + 32  // matchmaker
        + 32  // treasury
        + 2   // treasury_split_bps
        + 1; // bump
}
