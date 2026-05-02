use anchor_lang::prelude::*;

/// Per-client PDA used by `create_task` to enforce a per-client
/// task-creation rate limit (Phase 3 blocker #2).
///
/// Sliding window: at each `create_task` call, the program compares the
/// current `unix_timestamp` against `window_start_ts`. If the current
/// time is within the window
/// (`now - window_start_ts < RATE_LIMIT_WINDOW_SECONDS`), the existing
/// counter is incremented and checked against
/// `MAX_TASKS_PER_RATE_WINDOW`. Otherwise the window is reset
/// (start_ts → now, count → 1).
///
/// Created via `init_if_needed` so a client's first `create_task` pays
/// the rent (idempotent across subsequent calls). Anchor's
/// `init_if_needed` is allowed for this PDA because it holds no escrow
/// funds — only counters and a timestamp — so a PDA-resurrection
/// attack on a closed client account would gain the attacker nothing.
///
/// Seeds: `["client_state", client_pubkey]`
#[account]
pub struct ClientState {
    /// The wallet pubkey this state tracks.
    pub client: Pubkey,
    /// Unix timestamp (seconds) marking the start of the current
    /// rate-limit window. Set on first task creation, reset every time
    /// the window expires.
    pub window_start_ts: i64,
    /// Number of `create_task` calls landed in the current window.
    /// Capped by `crate::constants::MAX_TASKS_PER_RATE_WINDOW`.
    pub tasks_in_window: u32,
    /// Total tasks ever created by this client. Monotonic counter
    /// across all windows. Useful for off-chain analytics + future
    /// reputation signals; not load-bearing for the rate limit itself.
    pub total_tasks_created: u64,
    /// Reserved space for future fields without reallocation.
    pub _reserved: [u8; 32],
    pub bump: u8,
}

impl ClientState {
    pub const SPACE: usize = 8   // discriminator
        + 32  // client
        + 8   // window_start_ts
        + 4   // tasks_in_window
        + 8   // total_tasks_created
        + 32  // _reserved
        + 1; // bump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_state_space_is_93() {
        assert_eq!(ClientState::SPACE, 93);
    }
}
