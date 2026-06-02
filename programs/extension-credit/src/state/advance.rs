use anchor_lang::prelude::*;

/// A backer's advance to a recipient agent. The account **doubles as the
/// earnings-routing vault**: the recipient's Shillbot `payout_to` points at this
/// PDA, so finalized-task lamports accumulate here on top of the rent-exempt
/// minimum, and `route_and_recoup` splits them (backer first, recipient the
/// remainder).
///
/// Seeds: `["advance", backer, recipient]` — one active advance per
/// (backer, recipient) pair. `init` (not `init_if_needed`) prevents
/// resurrection while active; closed (recouped or defaulted) the pair can
/// open again.
#[account]
pub struct Advance {
    /// The backer who fronted the capital and is recouped first.
    pub backer: Pubkey,
    /// The funded agent; earnings route here and the remainder returns to it.
    pub recipient: Pubkey,
    /// Total capital fronted, in lamports — the amount owed before any split
    /// flows to the recipient.
    pub advance_lamports: u64,
    /// Cumulative lamports recouped to the backer so far.
    pub recouped_lamports: u64,
    /// Unix timestamp the advance was opened.
    pub created_at: i64,
    /// PDA bump.
    pub bump: u8,
    /// Reserved for future fields without reallocation.
    pub _reserved: [u8; 16],
}

impl Advance {
    // 8 + 32 + 32 + 8 + 8 + 8 + 1 + 16 = 113
    pub const SPACE: usize = 8 + 32 + 32 + 8 + 8 + 8 + 1 + 16;

    /// Lamports still owed to the backer.
    pub fn outstanding(&self) -> Result<u64> {
        Ok(self
            .advance_lamports
            .checked_sub(self.recouped_lamports)
            .ok_or(crate::errors::ExtensionCreditError::ArithmeticOverflow)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_space_is_113() {
        assert_eq!(Advance::SPACE, 113);
    }
}
