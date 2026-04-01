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
    /// Duration of the challenge window after verification (seconds).
    pub challenge_window_seconds: i64,
    /// Maximum time allowed for oracle verification after submission (seconds).
    pub verification_timeout_seconds: i64,
    /// Minimum delay between submission and attestation (seconds).
    pub attestation_delay_seconds: i64,
    /// Maximum age of oracle data relative to submission (seconds).
    pub staleness_window_seconds: i64,
    /// Maximum number of tasks an agent can have in Claimed state simultaneously.
    pub max_concurrent_claims: u8,
    /// Challenge bond multiplier in basis points (e.g., 20000 = 2x).
    pub challenge_bond_multiplier_bps: u16,
    /// Portion of slashed bond sent to treasury in basis points.
    pub bond_slash_treasury_bps: u16,
    /// Authority allowed to submit oracle attestations.
    pub oracle_authority: Pubkey,
    /// Whether the entire protocol is paused.
    pub paused: bool,
    /// Bitmask of paused platforms (bit N = PlatformType with value N).
    pub paused_platforms: u16,
    /// Switchboard pull feed account that provides oracle-attested composite scores.
    pub switchboard_feed: Pubkey,
    /// Reserved space for future fields without reallocation.
    pub _reserved: [u8; 32],
    pub bump: u8,
}

impl GlobalState {
    // 8 + 8 + 32 + 32 + 2 + 8 + 8 + 8 + 8 + 8 + 1 + 2 + 2 + 32 + 1 + 2 + 32 + 32 + 1 = 227
    pub const SPACE: usize = 8   // discriminator
        + 8    // task_counter
        + 32   // authority
        + 32   // treasury
        + 2    // protocol_fee_bps
        + 8    // quality_threshold
        + 8    // challenge_window_seconds
        + 8    // verification_timeout_seconds
        + 8    // attestation_delay_seconds
        + 8    // staleness_window_seconds
        + 1    // max_concurrent_claims
        + 2    // challenge_bond_multiplier_bps
        + 2    // bond_slash_treasury_bps
        + 32   // oracle_authority
        + 1    // paused
        + 2    // paused_platforms
        + 32   // switchboard_feed
        + 32   // _reserved
        + 1; // bump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_state_space_is_227() {
        assert_eq!(GlobalState::SPACE, 227);
    }
}
