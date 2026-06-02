use anchor_lang::prelude::*;

/// Registry-wide config: who arbitrates defaults and where slashed bonds go.
/// Set once by `initialize`. Replaces the earlier hardcoded `AUTHORITY` /
/// `TREASURY` constants so the authority + treasury are both testable (CI sets
/// its own wallet) and configurable (governance can rotate later).
///
/// Seeds: `["extension_global"]`
#[account]
pub struct GlobalState {
    /// Arbitrates defaults (`default_extension`).
    pub authority: Pubkey,
    /// Receives slashed bonds on default.
    pub treasury: Pubkey,
    /// PDA bump.
    pub bump: u8,
    /// Reserved for future fields without reallocation.
    pub _reserved: [u8; 32],
}

impl GlobalState {
    // 8 + 32 + 32 + 1 + 32 = 105
    pub const SPACE: usize = 8 + 32 + 32 + 1 + 32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_state_space_is_105() {
        assert_eq!(GlobalState::SPACE, 105);
    }
}
