//! Compile-time constants for the Shillbot program.

/// Per-client `create_task` rate-limit window. Used as the INITIAL value
/// for `GlobalState.rate_limit_window_seconds`; multisig governance can
/// adjust at runtime via `update_params` within
/// `[MIN_RATE_LIMIT_WINDOW_SECONDS, MAX_RATE_LIMIT_WINDOW_SECONDS]`.
pub const RATE_LIMIT_WINDOW_SECONDS: i64 = 3_600;

/// Per-window task-creation cap. INITIAL value for
/// `GlobalState.max_tasks_per_rate_window`; bounds enforced at update time.
pub const MAX_TASKS_PER_RATE_WINDOW: u32 = 10;

pub const MIN_RATE_LIMIT_WINDOW_SECONDS: i64 = 60;
pub const MAX_RATE_LIMIT_WINDOW_SECONDS: i64 = 86_400;

pub const MIN_TASKS_PER_RATE_WINDOW: u32 = 1;
pub const MAX_TASKS_PER_RATE_WINDOW_CEILING: u32 = 100;

/// Minimum escrow for `PlatformType::OnChainAction` (`platform == 11`) tasks (`0.005 SOL`).
pub const MIN_CAMPAIGN_ESCROW_LAMPORTS: u64 = 5_000_000;

/// Minimum effective attestation delay for `OnChainAction` tasks (`60` seconds).
pub const MIN_ONCHAIN_ACTION_ATTESTATION_DELAY_SECONDS: u32 = 60;

/// Maximum task deadline horizon from creation timestamp (`365` days).
pub const MAX_TASK_DEADLINE_HORIZON_SECONDS: i64 = 365 * 86_400;

/// Maximum deadline horizon for `OnChainAction` campaigns (`90` days).
pub const MAX_ONCHAIN_ACTION_DEADLINE_HORIZON_SECONDS: i64 = 90 * 86_400;

/// Strike cooldown imposed on an agent after a score-0 reopen (`60` seconds).
pub const SCORE_ZERO_COOLDOWN_SECONDS: i64 = 60;

/// Canonical `extension-credit` program ID (`Advance` splitter vaults).
pub const EXTENSION_CREDIT_PROGRAM_ID: anchor_lang::prelude::Pubkey =
    anchor_lang::prelude::Pubkey::from_str_const("Ec628D7GH3hwgnVf1gqrUh83qcZprYjmTzvtdHHEr7oh");

/// `Advance` account size in `extension-credit` (`8 + 32 + 32 + 8 + 8 + 8 + 1 + 16 = 113`).
pub const EXTENSION_CREDIT_ADVANCE_SPACE: usize = 113;
