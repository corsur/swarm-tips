#![allow(dead_code)]

/// Maximum composite score value (fixed-point 1e6).
pub const MAX_SCORE: u64 = 1_000_000;

/// Number of scoring metrics.
pub const METRIC_COUNT: usize = 6;

/// Basis points denominator (100% = 10_000 bps).
pub const BPS_DENOMINATOR: u16 = 10_000;

/// Minimum allowed value for a single scoring weight (basis points).
/// 500 bps = 5%.
pub const MIN_WEIGHT_BPS: u16 = 500;

/// Maximum allowed value for a single scoring weight (basis points).
/// 5000 bps = 50%.
pub const MAX_WEIGHT_BPS: u16 = 5_000;

/// Minimum protocol fee in basis points (1%).
pub const MIN_PROTOCOL_FEE_BPS: u16 = 100;

/// Maximum protocol fee in basis points (25%).
pub const MAX_PROTOCOL_FEE_BPS: u16 = 2_500;

/// Maximum penalty weight in basis points.
/// Penalty weight is separate from the 6 metric weights and not included in their sum.
pub const MAX_PENALTY_WEIGHT_BPS: u16 = 5_000;
