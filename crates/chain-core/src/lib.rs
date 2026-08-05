#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

//! Chain-agnostic core types for the multichain standard
//! (`swarm/CLAUDE.md` "Multichain Frameworks", `multichain/decision.md`).
//!
//! This crate is types-only: no RPC clients, no async, no chain SDKs.
//! It is consumed by the Anchor program (BPF), backend services, and
//! tests, so its default feature set has zero dependencies.

pub mod caip;
pub mod cert_schema;
#[cfg(feature = "cosign")]
pub mod cosign;
pub mod game;
pub mod verify_schema;
pub mod words;

pub use caip::{AccountId, ChainId, Namespace};
