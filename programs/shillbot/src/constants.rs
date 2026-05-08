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
