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

/// Minimum task escrow in lamports (Phase 3 blocker #2).
///
/// 0.36 SOL ≈ $50 at $140/SOL, the dollar floor named in the v4 roadmap
/// for rejecting trivial sybil farming. Rationale:
///
/// A sybil attacker who controls both client and agent wallets pays the
/// escrow on `create_task` and recovers most of it on `finalize_task`,
/// minus `protocol_fee_bps` (default 100 bps = 1%). At MIN_ESCROW =
/// 0.36 SOL and protocol fee 1%, each round-trip costs ~$0.50 in
/// protocol fees plus ~8 days of locked capital (7d verification +
/// 1d challenge window). At the per-client rate limit below, 10 tasks
/// per hour means 3.6 SOL of locked capital per hour, ceiling about
/// 240 SOL across an 8-day window — non-trivial for a marginal sybil
/// operation. See `programs/shillbot/CLAUDE.md` "Sybil economics" for
/// the full attack-cost analysis.
///
/// `const`, not a config field on `GlobalState`: per the v4 roadmap
/// "must be a `const`, not a config field, so it can't be downgraded
/// silently."
pub const MIN_ESCROW_LAMPORTS: u64 = 360_000_000;

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
// D2/D3 governance bounds (2026-05-07)
// ---------------------------------------------------------------------------
// `MIN_ESCROW_LAMPORTS`, `RATE_LIMIT_WINDOW_SECONDS`, and
// `MAX_TASKS_PER_RATE_WINDOW` moved from compile-time consts to
// `GlobalState` governance params. The consts above remain as INITIAL
// values used by `initialize`. After deploy, multisig governance can
// adjust within the bounds below via `update_params` — preserving the
// "can't be downgraded silently" property by enforcing floors that
// keep sybil-resistance meaningful even at the lower bound.

/// Minimum allowed `min_escrow_lamports` value. 0.05 SOL ≈ $7-8 at
/// $140/SOL; sybil per-task cost stays at ~$0.07 protocol fee even at
/// this lower bound — materially weaker than the 0.36 SOL default but
/// still non-trivial friction.
pub const MIN_ESCROW_LAMPORTS_FLOOR: u64 = 50_000_000;
/// Maximum allowed `min_escrow_lamports` value. 1 SOL = ~$140 at
/// $140/SOL; prevents governance from setting an absurdly high floor
/// that excludes legitimate clients.
pub const MIN_ESCROW_LAMPORTS_CEILING: u64 = 1_000_000_000;

/// Rate-limit window bounds: [1 minute, 1 day].
pub const MIN_RATE_LIMIT_WINDOW_SECONDS: i64 = 60;
pub const MAX_RATE_LIMIT_WINDOW_SECONDS: i64 = 86_400;

/// Per-window task cap bounds: [1, 100].
pub const MIN_TASKS_PER_RATE_WINDOW: u32 = 1;
pub const MAX_TASKS_PER_RATE_WINDOW_CEILING: u32 = 100;
