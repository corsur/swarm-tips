//! Program constants.

/// Minimum advance, in lamports (0.001 SOL). Keeps advances small + fast to
/// recoup (the mitigation for the earnings-leakage limit). SOL for the MVP.
pub const MIN_ADVANCE_LAMPORTS: u64 = 1_000_000;
