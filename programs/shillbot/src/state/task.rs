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
/// Submitted --(approve_task: client signs)--> Approved
/// Submitted --(expire_task: T+14d timeout)--> [closed]
/// Approved --(verify_task: oracle authority)--> Verified
/// Approved --(expire_task: T+14d timeout)--> [closed]
/// Verified --(finalize_task)--> Finalized --> [closed]
/// Verified --(challenge_task)--> Disputed
/// Disputed --(resolve_challenge)--> Resolved --> [closed]
/// ```
///
/// Note: `Approved = 7` is appended to preserve `#[repr(u8)]` discriminants
/// of all prior variants. Existing on-chain Task accounts (created before
/// the Phase 3 blocker #3a upgrade) keep their bytewise interpretation;
/// new variants must always append.
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
    Approved = 7,
}

#[account]
pub struct Task {
    pub task_id: u64,
    pub client: Pubkey,
    /// Zero-key until claimed.
    pub agent: Pubkey,
    pub state: TaskState,
    /// Platform this task targets (PlatformType discriminant).
    pub platform: u8,
    /// Client's escrowed payment (lamports).
    pub escrow_lamports: u64,
    /// SHA-256 of the off-chain campaign brief.
    pub content_hash: [u8; 32],
    /// SHA-256 of submitted content ID (zeroed until submitted).
    pub content_id_hash: [u8; 32],
    /// Random nonce the agent must include in the content.
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
    /// Per-task attestation delay override in seconds. 0 = use GlobalState default.
    pub attestation_delay_override: u32,
    /// Per-task challenge window override in seconds. 0 = use GlobalState default.
    pub challenge_window_override: u32,
    /// Per-task verification timeout override in seconds. 0 = use GlobalState default.
    pub verification_timeout_override: u32,
    /// SHA-256 of the verification snapshot (content + metrics JSON). 0 until verified.
    pub verification_hash: [u8; 32],
    /// Reserved space for future fields without reallocation.
    pub _reserved: [u8; 20],
    pub bump: u8,
}

impl Task {
    // 8 + 8 + 32 + 32 + 1 + 1 + 8 + 32 + 32 + 16 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 64 + 1 = 315
    pub const SPACE: usize = 8   // discriminator
        + 8    // task_id
        + 32   // client
        + 32   // agent
        + 1    // state (enum tag)
        + 1    // platform
        + 8    // escrow_lamports
        + 32   // content_hash
        + 32   // content_id_hash
        + 16   // task_nonce
        + 8    // composite_score
        + 8    // payment_amount
        + 8    // fee_amount
        + 8    // deadline
        + 8    // submit_margin
        + 8    // claim_buffer
        + 8    // created_at
        + 8    // submitted_at
        + 8    // verified_at
        + 8    // challenge_deadline
        + 4    // attestation_delay_override
        + 4    // challenge_window_override
        + 4    // verification_timeout_override
        + 32   // verification_hash
        + 20   // _reserved
        + 1; // bump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_space_is_315() {
        assert_eq!(Task::SPACE, 315);
    }

    #[test]
    fn task_state_values_are_distinct() {
        // Use a HashSet to prove all variants have distinct discriminants
        // (the prior pairwise asserts were O(n) and missed cross-pairs).
        use std::collections::HashSet;
        let all = [
            TaskState::Open,
            TaskState::Claimed,
            TaskState::Submitted,
            TaskState::Verified,
            TaskState::Finalized,
            TaskState::Disputed,
            TaskState::Resolved,
            TaskState::Approved,
        ];
        let set: HashSet<u8> = all.iter().map(|s| *s as u8).collect();
        assert_eq!(
            set.len(),
            all.len(),
            "TaskState discriminants must all be distinct"
        );
    }

    #[test]
    fn task_state_repr_matches_expected() {
        // CRITICAL: Mainnet on-chain Task accounts encode `state` as the
        // u8 discriminant of this enum. Reordering or renumbering ANY
        // prior variant breaks bytewise interpretation of existing
        // accounts. New variants must always be appended at the end.
        assert_eq!(TaskState::Open as u8, 0);
        assert_eq!(TaskState::Claimed as u8, 1);
        assert_eq!(TaskState::Submitted as u8, 2);
        assert_eq!(TaskState::Verified as u8, 3);
        assert_eq!(TaskState::Finalized as u8, 4);
        assert_eq!(TaskState::Disputed as u8, 5);
        assert_eq!(TaskState::Resolved as u8, 6);
        assert_eq!(TaskState::Approved as u8, 7);
    }
}
