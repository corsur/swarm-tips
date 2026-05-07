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

// MIN_ESCROW_LAMPORTS (and the bound consts MIN_ESCROW_LAMPORTS_FLOOR,
// MIN_ESCROW_LAMPORTS_CEILING) removed 2026-05-07. The 0.36 SOL per-task
// escrow floor (Phase 3 blocker #2 from the 2026-04-02 panel review)
// imposed real friction on legitimate small clients to deter sybil
// round-trips at solo + pre-PMF scale. The right end-state defense is the
// EigenTrust reputation graph (sybil clusters self-vouch and gain zero
// global trust); see `swarm-tips/CLAUDE.md` Phase 2. The vestigial
// `GlobalState.min_escrow_lamports` slot is preserved for binary compat
// with deployed 227-byte accounts; no instruction reads it.

/// Per-client task-creation rate-limit window (Phase 3 blocker #2).
/// Sliding window of 1 hour: a client can `create_task` at most
/// `MAX_TASKS_PER_RATE_WINDOW` times within any 1-hour window. Window
/// resets when the next `create_task` lands more than this many seconds
/// after the current window's start.
pub const RATE_LIMIT_WINDOW_SECONDS: i64 = 3_600;

/// Maximum task-creations allowed per `RATE_LIMIT_WINDOW_SECONDS` per
/// client. Caps a single client's task-creation throughput; sybil
/// attackers must spawn additional client wallets to exceed it. Each
/// new wallet pays a small (~$0.13) one-time `ClientState` rent in
/// addition to the recurring per-task fee bleed (~$0.50/task at 1%
/// fee on the $50 escrow floor) — the rate limit's primary effect is
/// forcing attackers to maintain more wallets, not the rent itself.
pub const MAX_TASKS_PER_RATE_WINDOW: u32 = 10;

// ---------------------------------------------------------------------------
// D3 governance bounds (2026-05-07)
// ---------------------------------------------------------------------------
// `RATE_LIMIT_WINDOW_SECONDS` and `MAX_TASKS_PER_RATE_WINDOW` moved from
// compile-time consts to `GlobalState` governance params. The consts above
// remain as INITIAL values used by `initialize`. After deploy, multisig
// governance can adjust within the bounds below via `update_params`.

/// Rate-limit window bounds: [1 minute, 1 day].
pub const MIN_RATE_LIMIT_WINDOW_SECONDS: i64 = 60;
pub const MAX_RATE_LIMIT_WINDOW_SECONDS: i64 = 86_400;

/// Per-window task cap bounds: [1, 100].
pub const MIN_TASKS_PER_RATE_WINDOW: u32 = 1;
pub const MAX_TASKS_PER_RATE_WINDOW_CEILING: u32 = 100;
