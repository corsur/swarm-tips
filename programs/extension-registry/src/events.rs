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

/// Emitted when the extender withdraws its vouch (reclaiming bond + rent). The
/// off-chain web-position indexer treats this as removal of the directed edge
/// extender → recipient, so the recipient's borrowed standing recomputes.
#[event]
pub struct ExtensionWithdrawn {
    pub extender: Pubkey,
    pub recipient: Pubkey,
    pub bond_lamports: u64,
}

/// Emitted when an extender initiates the 7-day unbond cooldown (`request_unbond`).
/// While unbonding, the vouch no longer qualifies for new `extension-credit`
/// advances and remains slashable via `default_extension` until withdrawn.
#[event]
pub struct UnbondRequested {
    pub extender: Pubkey,
    pub recipient: Pubkey,
    pub requested_at: i64,
    pub withdrawable_at: i64,
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
