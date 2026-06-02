use anchor_lang::prelude::*;

/// Emitted once when the registry is configured.
#[event]
pub struct GlobalInitialized {
    pub authority: Pubkey,
    pub treasury: Pubkey,
}

/// Emitted when an extender locks a bond and creates an obligation edge.
/// The off-chain web-position indexer treats this as a directed edge
/// extender → recipient, weighted by the extender's own standing.
#[event]
pub struct ExtensionSubmitted {
    pub extender: Pubkey,
    pub recipient: Pubkey,
    pub extension_type: u8,
    pub bond_lamports: u64,
    pub created_at: i64,
}

/// Emitted when the extender attests the recipient fulfilled the obligation.
/// Positive terminal signal — the edge resolved successfully.
#[event]
pub struct ReturnSubstanceAttested {
    pub extender: Pubkey,
    pub recipient: Pubkey,
    pub bond_lamports: u64,
}

/// Emitted when the authority records a default. Negative terminal signal —
/// the bond was slashed to the treasury; feeds default-detection / the
/// failure-records archive.
#[event]
pub struct ExtensionDefaulted {
    pub extender: Pubkey,
    pub recipient: Pubkey,
    pub bond_lamports_slashed: u64,
}
