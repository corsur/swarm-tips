#![deny(warnings)]
#![deny(clippy::all)]

//! Shared on-chain game operations for the coordination game.
//!
//! Extracts PDA derivation, instruction building, commit-reveal logic,
//! and high-level RPC client operations from the grok-agent so that
//! multiple services and agents can reuse them.

pub mod client;
pub mod commit;
pub mod instructions;
pub mod pda;

/// Re-export coordination program types that callers need.
pub use coordination::state::{Game, GameState, GlobalConfig};
