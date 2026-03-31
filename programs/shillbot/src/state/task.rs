use anchor_lang::prelude::*;

/// Task lifecycle states.
///
/// ```text
///          --(create_task)--> Open
/// Open --(claim_task)--> Claimed
/// Open --(expire_task: past deadline)--> [closed]
/// Open --(emergency_return)--> [closed]
/// Claimed --(submit_work)--> Submitted
/// Claimed --(expire_task: past deadline)--> [closed]
/// Claimed --(emergency_return)--> [closed]
/// Submitted --(verify_task)--> Verified
/// Submitted --(expire_task: T+14d timeout)--> [closed]
/// Verified --(finalize_task)--> Finalized --> [closed]
/// Verified --(challenge_task)--> Disputed
/// Disputed --(resolve_challenge)--> Resolved --> [closed]
/// ```
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TaskState {
    Open = 0,
    Claimed = 1,
    Submitted = 2,
    Verified = 3,
    Finalized = 4,
    Disputed = 5,
    Resolved = 6,
}

#[account]
pub struct Task {
    pub task_id: u64,
    pub client: Pubkey,
    /// Zero-key until claimed.
    pub agent: Pubkey,
    pub state: TaskState,
    /// Client's escrowed payment (lamports).
    pub escrow_lamports: u64,
    /// SHA-256 of the off-chain campaign brief.
    pub content_hash: [u8; 32],
    /// SHA-256 of submitted YouTube video ID (zeroed until submitted).
    pub video_id_hash: [u8; 32],
    /// Random nonce the agent must include in the video description.
    pub task_nonce: [u8; 16],
    /// Fixed-point score from oracle attestation (0 until verified).
    pub composite_score: u64,
    /// Computed payment amount (0 until verified).
    pub payment_amount: u64,
    /// Computed protocol fee amount (0 until verified). Stored at verification time
    /// so finalize_task and resolve_challenge use the fee that was in effect when
    /// the oracle attested, preventing parameter-change bricking (S-03).
    pub fee_amount: u64,
    pub deadline: i64,
    /// Seconds before deadline that submission must occur.
    pub submit_margin: i64,
    /// Minimum seconds remaining to claim.
    pub claim_buffer: i64,
    pub created_at: i64,
    /// 0 until submitted.
    pub submitted_at: i64,
    /// 0 until oracle attestation.
    pub verified_at: i64,
    /// 0 until challenge window starts.
    pub challenge_deadline: i64,
    pub bump: u8,
}

impl Task {
    pub const SPACE: usize = 8  // discriminator
        + 8   // task_id
        + 32  // client
        + 32  // agent
        + 1   // state (enum tag)
        + 8   // escrow_lamports
        + 32  // content_hash
        + 32  // video_id_hash
        + 16  // task_nonce
        + 8   // composite_score
        + 8   // payment_amount
        + 8   // fee_amount
        + 8   // deadline
        + 8   // submit_margin
        + 8   // claim_buffer
        + 8   // created_at
        + 8   // submitted_at
        + 8   // verified_at
        + 8   // challenge_deadline
        + 1; // bump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_state_values_are_distinct() {
        assert_ne!(TaskState::Open as u8, TaskState::Claimed as u8);
        assert_ne!(TaskState::Claimed as u8, TaskState::Submitted as u8);
        assert_ne!(TaskState::Submitted as u8, TaskState::Verified as u8);
        assert_ne!(TaskState::Verified as u8, TaskState::Finalized as u8);
        assert_ne!(TaskState::Finalized as u8, TaskState::Disputed as u8);
        assert_ne!(TaskState::Disputed as u8, TaskState::Resolved as u8);
    }

    #[test]
    fn task_state_repr_matches_expected() {
        assert_eq!(TaskState::Open as u8, 0);
        assert_eq!(TaskState::Claimed as u8, 1);
        assert_eq!(TaskState::Submitted as u8, 2);
        assert_eq!(TaskState::Verified as u8, 3);
        assert_eq!(TaskState::Finalized as u8, 4);
        assert_eq!(TaskState::Disputed as u8, 5);
        assert_eq!(TaskState::Resolved as u8, 6);
    }
}
