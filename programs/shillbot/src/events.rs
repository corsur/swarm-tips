use anchor_lang::prelude::*;

#[event]
pub struct TaskCreated {
    pub task_id: u64,
    pub client: Pubkey,
    pub escrow_lamports: u64,
    pub deadline: i64,
    pub task_nonce: [u8; 16],
    pub platform: u8,
}

#[event]
pub struct TaskClaimed {
    pub task_id: u64,
    pub agent: Pubkey,
}

#[event]
pub struct WorkSubmitted {
    pub task_id: u64,
    pub agent: Pubkey,
    pub content_id_hash: [u8; 32],
}

#[event]
pub struct TaskVerified {
    pub task_id: u64,
    pub composite_score: u64,
    pub payment_amount: u64,
    pub fee_amount: u64,
    pub verification_hash: [u8; 32],
}

#[event]
pub struct TaskFinalized {
    pub task_id: u64,
    pub agent: Pubkey,
    pub payment_amount: u64,
    pub fee_amount: u64,
}

#[event]
pub struct TaskChallenged {
    pub task_id: u64,
    pub challenger: Pubkey,
    pub bond_lamports: u64,
    pub is_client_challenge: bool,
}

#[event]
pub struct ChallengeResolved {
    pub task_id: u64,
    pub challenger_won: bool,
    pub bond_slashed: u64,
}

#[event]
pub struct TaskExpired {
    pub task_id: u64,
    pub state_at_expiry: u8,
    pub platform: u8,
}

#[event]
pub struct EmergencyReturn {
    pub task_ids: Vec<u64>,
}

#[event]
pub struct SessionCreated {
    pub agent: Pubkey,
    pub delegate: Pubkey,
    pub allowed_instructions: u8,
}

#[event]
pub struct SessionRevoked {
    pub agent: Pubkey,
    pub delegate: Pubkey,
}

#[event]
pub struct ParamsUpdated {
    pub protocol_fee_bps: u16,
    pub quality_threshold: u64,
}

#[event]
pub struct AgentStateClosed {
    pub agent: Pubkey,
}

#[event]
pub struct AuthorityTransferred {
    pub old_authority: Pubkey,
    pub new_authority: Pubkey,
}

#[event]
pub struct TreasuryUpdated {
    pub old_treasury: Pubkey,
    pub new_treasury: Pubkey,
}

#[event]
pub struct OracleAuthorityUpdated {
    pub old_oracle_authority: Pubkey,
    pub new_oracle_authority: Pubkey,
}

#[event]
pub struct IdentityRegistered {
    pub agent: Pubkey,
    pub platform: u8,
    pub identity_hash: [u8; 32],
}

#[event]
pub struct IdentityRevoked {
    pub agent: Pubkey,
    pub platform: u8,
}
