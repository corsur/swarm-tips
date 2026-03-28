use anchor_lang::prelude::*;

#[account]
pub struct GlobalState {
    /// Monotonic counter incremented on each create_task.
    pub task_counter: u64,
    /// Squads multisig (mainnet) or EOA (devnet).
    pub authority: Pubkey,
    /// Treasury account for protocol fee collection.
    pub treasury: Pubkey,
    /// Protocol fee in basis points (100 = 1%).
    pub protocol_fee_bps: u16,
    /// Minimum composite score for payment (fixed-point, max = MAX_SCORE).
    pub quality_threshold: u64,
    pub bump: u8,
}

impl GlobalState {
    pub const SPACE: usize = 8  // discriminator
        + 8   // task_counter
        + 32  // authority
        + 32  // treasury
        + 2   // protocol_fee_bps
        + 8   // quality_threshold
        + 1; // bump
}
