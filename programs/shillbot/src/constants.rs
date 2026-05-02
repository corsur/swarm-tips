//! Compile-time constants for the Shillbot program.
//!
//! Replaces fields that were previously authority-mutable via dedicated
//! "setter" instructions. Locking these as `const` removes a class of
//! single-key compromise risk: an attacker who steals the authority
//! keypair can no longer point the program at an attacker-controlled
//! Switchboard feed (which would let them post arbitrary scores and
//! drain task escrows).

use anchor_lang::prelude::*;

/// Switchboard pull feed account that provides oracle-attested
/// composite scores. Verified at every `verify_task` call against the
/// account passed by the caller.
///
/// **USER MUST FILL before mainnet program upgrade:** the value below is
/// a placeholder (System Program + 1). The real production feed pubkey
/// must be inserted here, the program rebuilt, and the upgrade signed
/// by the upgrade authority. Without this swap, `verify_task` will
/// silently reject every attestation because the feed account passed
/// in won't match the placeholder.
///
/// Test setups that exercise `verify_task` create a bankrun account at
/// this exact pubkey so the validation passes — see
/// `tests/shillbot-lifecycle.ts` for the pattern.
pub const SWITCHBOARD_FEED: Pubkey = pubkey!("11111111111111111111111111111112");
