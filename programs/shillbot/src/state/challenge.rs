use anchor_lang::prelude::*;

#[account]
pub struct Challenge {
    pub task_id: u64,
    pub challenger: Pubkey,
    pub bond_lamports: u64,
    /// True if challenger is the task's client.
    pub is_client_challenge: bool,
    pub created_at: i64,
    pub resolved: bool,
    pub challenger_won: bool,
    /// Reserved space for future fields without reallocation.
    pub _reserved: [u8; 32],
    pub bump: u8,
}

impl Challenge {
    // 8 + 8 + 32 + 8 + 1 + 8 + 1 + 1 + 32 + 1 = 100
    pub const SPACE: usize = 8   // discriminator
        + 8    // task_id
        + 32   // challenger
        + 8    // bond_lamports
        + 1    // is_client_challenge
        + 8    // created_at
        + 1    // resolved
        + 1    // challenger_won
        + 32   // _reserved
        + 1; // bump
}

#[account]
pub struct SessionDelegate {
    /// The agent who delegated.
    pub agent: Pubkey,
    /// The session key (MCP server holds this).
    pub delegate: Pubkey,
    /// Bitmask: 0x01 = claim_task, 0x02 = submit_work.
    pub allowed_instructions: u8,
    pub created_at: i64,
    /// Unix timestamp when this session delegation expires (0 = no expiry).
    pub expires_at: i64,
    /// Reserved space for future fields without reallocation.
    pub _reserved: [u8; 32],
    pub bump: u8,
}

impl SessionDelegate {
    /// Permission bit: session key may call `claim_task_session`.
    pub const CLAIM_TASK_BIT: u8 = 0x01;
    /// Permission bit: session key may call `submit_work_session`.
    pub const SUBMIT_WORK_BIT: u8 = 0x02;
    /// All bits with defined meaning. `create_session` rejects any grant
    /// outside this mask (reject at the boundary — see that handler).
    pub const DEFINED_INSTRUCTION_BITS: u8 = Self::CLAIM_TASK_BIT | Self::SUBMIT_WORK_BIT;

    // 8 + 32 + 32 + 1 + 8 + 8 + 32 + 1 = 122
    pub const SPACE: usize = 8   // discriminator
        + 32   // agent
        + 32   // delegate
        + 1    // allowed_instructions
        + 8    // created_at
        + 8    // expires_at
        + 32   // _reserved
        + 1; // bump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_space_is_100() {
        assert_eq!(Challenge::SPACE, 100);
    }

    #[test]
    fn defined_session_bits_are_claim_and_submit_only() {
        assert_eq!(SessionDelegate::DEFINED_INSTRUCTION_BITS, 0x03);
        // Any bit outside the mask must trip the create_session boundary check.
        for undefined_bit in [0x04u8, 0x08, 0x10, 0x20, 0x40, 0x80] {
            assert_ne!(
                undefined_bit & !SessionDelegate::DEFINED_INSTRUCTION_BITS,
                0
            );
        }
    }

    #[test]
    fn session_delegate_space_is_122() {
        assert_eq!(SessionDelegate::SPACE, 122);
    }
}
