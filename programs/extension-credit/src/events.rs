use anchor_lang::prelude::*;

/// Emitted when a backer fronts capital and opens an advance.
#[event]
pub struct AdvanceOpened {
    pub backer: Pubkey,
    pub recipient: Pubkey,
    pub advance_lamports: u64,
    pub created_at: i64,
}

/// Emitted each time the crank distributes routed earnings.
#[event]
pub struct AdvanceRecouped {
    pub backer: Pubkey,
    pub recipient: Pubkey,
    pub to_backer: u64,
    pub to_recipient: u64,
    pub total_recouped: u64,
    pub advance_lamports: u64,
}

/// Emitted when a fully-recouped advance is closed.
#[event]
pub struct AdvanceClosed {
    pub backer: Pubkey,
    pub recipient: Pubkey,
}

/// Emitted when a backer marks a default and sweeps the vault.
#[event]
pub struct AdvanceDefaulted {
    pub backer: Pubkey,
    pub recipient: Pubkey,
    pub swept_to_backer: u64,
    pub total_recouped: u64,
    pub advance_lamports: u64,
}
