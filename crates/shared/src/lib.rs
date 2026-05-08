#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

pub mod constants;
pub mod platform;
pub mod scoring;

pub use constants::*;
pub use platform::{EngagementMetrics, PlatformProof, PlatformType, ScoringStyle};
pub use scoring::{CompositeScore, ScoringWeights};
