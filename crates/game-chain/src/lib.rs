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
///
/// `DEFAULT_STAKE_LAMPORTS` is DELIBERATELY NOT re-exported. It is the program's
/// compile-time *initialization* default (0.05 SOL), not the live stake, and
/// exporting it made it reachable as a second source of truth. mcp-server used
/// it to build `create_game`, so once mainnet was re-pegged to 68,482,585 every
/// game creation failed with `StakeMismatch` — no game has been created on
/// mainnet since 2026-07-30.
///
/// The live value is `GlobalConfig.stake_lamports`, read from the chain. Callers
/// get it from `client::GameChainClient::stake_lamports()`; `build_create_game`
/// reads it internally so the value cannot be supplied wrongly at all.
/// Keeping this un-exported makes the compiler the enforcement.
pub use coordination::instructions::xchain::CreateXMatchArgs;
pub use coordination::state::{Game, GameState, GlobalConfig};
