//! `migrate_agent_state` — one-shot, agent-paid realloc of legacy `AgentState`
//! PDAs from the original 42-byte v1 layout to the current 90-byte layout.
//!
//! Background. Sprint 0 of the v5 roadmap-iteration discovered (2026-05-06)
//! that `AgentState` PDAs created against the v1 program shape
//! (commit 2f294b2, March 2026: `{ agent: Pubkey, claimed_count: u8, bump:
//! u8 }`) sit on disk at `8 disc + 32 + 1 + 1 = 42` bytes. The current
//! struct expects 90 bytes, so Anchor's typed deserializer fails on these
//! legacy accounts with `AccountDidNotDeserialize` (3003) — observed for the
//! founder wallet `CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY` on devnet,
//! PDA `67NVWtF4j1niHXR4zBRRBytru8TFC3959u6SfhCE5btn`. The v3 reputation
//! extension (commit 713059c, 2026-05-02) was bytewise-compatible because it
//! kept SPACE at 90 and only re-carved a `_reserved[32]` block; the v1→v2
//! jump (commit 996923b, the `total_completed` + `total_earned` + 32-byte
//! reserved expansion) is what introduced the on-disk size mismatch.
//!
//! Design.
//!   * The instruction takes the legacy account as a raw `AccountInfo` —
//!     `Account<AgentState>` would fail to open the underlying 42-byte data
//!     before the handler runs.
//!   * Discriminator + PDA seeds + signer match are all verified explicitly,
//!     standing in for the automatic checks `Account<...>` normally performs.
//!   * The legacy bump is at offset 41 and the legacy claimed_count is at
//!     offset 40; the new layout puts claimed_count at offset 40 (same!)
//!     and bump at offset 89. We extract both before resize and write them
//!     back as part of the new struct.
//!   * `claimed_count` is PRESERVED across the migration — it represents
//!     active outstanding claims that aren't safe to silently zero. Agents
//!     with stale counts (claims whose tasks finalized via legacy paths
//!     that didn't decrement) can issue a separate close + re-init flow.
//!   * Idempotent for already-migrated accounts: a 90-byte account that
//!     deserializes cleanly returns `Ok(())` so retried txs don't fail.
//!   * **Caller pre-funds the rent diff.** This handler does not do a
//!     SystemProgram::transfer CPI. The agent's client-side tx must
//!     include a transfer instruction before this one, sized at
//!     `>= rent.minimum_balance(90) - current_lamports`. Keeping the
//!     handler pure-compute simplifies the borrow lifetime around
//!     `AccountInfo::resize`. The two-instruction client UX is
//!     acceptable for a one-shot migration.
//!
//! Layout transition (the actual bytewise contract):
//! ```text
//! v1 (42 bytes total):                      NEW (90 bytes total):
//!   [0..8]   discriminator                    [0..8]   discriminator
//!   [8..40]  agent (32)                       [8..40]  agent (32)
//!   [40]     claimed_count (1)                [40]     claimed_count (preserved)
//!   [41]     bump (1)                         [41..49] total_completed = 0
//!   --                                        [49..57] total_earned = 0
//!                                             [57..65] total_score_sum = 0
//!                                             [65..73] total_tasks_claimed = 0
//!                                             [73..81] total_challenges_lost = 0
//!                                             [81..89] _reserved = [0; 8]
//!                                             [89]     bump (preserved)
//! ```

use anchor_lang::prelude::*;
use anchor_lang::Discriminator;

use crate::errors::ShillbotError;
use crate::state::AgentState;

const LEGACY_AGENT_STATE_SIZE: usize = 8 + 32 + 1 + 1; // 42 (v1 layout)
const CURRENT_AGENT_STATE_SIZE: usize = AgentState::SPACE; // 90

/// One-shot migration of a 42-byte v1 `AgentState` PDA to the current
/// 90-byte layout. Idempotent on already-migrated accounts.
pub fn migrate_agent_state(ctx: Context<MigrateAgentState>) -> Result<()> {
    let agent_state_info = &ctx.accounts.agent_state;
    let agent_signer = &ctx.accounts.agent;
    // Checks: ownership (defense-in-depth — Anchor only enforces ownership
    // when the account is typed as `Account<T>`, and we use AccountInfo).
    require_keys_eq!(
        *agent_state_info.owner,
        crate::ID,
        ShillbotError::AgentStateDiscriminatorMismatch
    );
    let current_size = agent_state_info.data_len();
    if current_size == CURRENT_AGENT_STATE_SIZE {
        return migrate_already_current(agent_state_info, agent_signer.key());
    }
    // Only the v1 42-byte case is supported. Truncated / partial-migrated
    // / corrupted accounts are rejected.
    require!(
        current_size == LEGACY_AGENT_STATE_SIZE,
        ShillbotError::AgentStateUnsupportedSize
    );
    let (old_agent, old_claimed_count, old_bump) = read_v1_fields(agent_state_info)?;
    require_keys_eq!(
        old_agent,
        agent_signer.key(),
        ShillbotError::AgentStateAgentMismatch
    );
    // The caller-side tx must include a SystemProgram::transfer before
    // this ix sized at >= rent.minimum_balance(90) - current. Keeping the
    // handler pure-compute simplifies the borrow lifetime around resize.
    let new_min_lamports = Rent::get()?.minimum_balance(CURRENT_AGENT_STATE_SIZE);
    require!(
        agent_state_info.lamports() >= new_min_lamports,
        ShillbotError::AgentStateUnsupportedSize
    );
    // Effects: resize + write the new layout. claimed_count is preserved
    // (active outstanding claims aren't safe to silently zero).
    agent_state_info.resize(CURRENT_AGENT_STATE_SIZE)?;
    write_current_layout(agent_state_info, old_agent, old_claimed_count, old_bump)?;
    msg!(
        "migrated AgentState {} from {} to {} bytes",
        agent_signer.key(),
        LEGACY_AGENT_STATE_SIZE,
        CURRENT_AGENT_STATE_SIZE
    );
    Ok(())
}

/// Idempotency path: account is already at the current size — confirm it
/// deserializes and the agent pubkey matches the signer, then exit clean.
fn migrate_already_current(agent_state_info: &AccountInfo, agent_signer_key: Pubkey) -> Result<()> {
    let data = agent_state_info.try_borrow_data()?;
    require!(
        data.len() >= 8 && data[..8] == *AgentState::DISCRIMINATOR,
        ShillbotError::AgentStateDiscriminatorMismatch
    );
    let parsed = AgentState::try_deserialize(&mut &data[..])
        .map_err(|_| error!(ShillbotError::AgentStateDiscriminatorMismatch))?;
    require_keys_eq!(
        parsed.agent,
        agent_signer_key,
        ShillbotError::AgentStateAgentMismatch
    );
    Ok(())
}

/// Decode the v1 42-byte layout: discriminator [0..8], agent [8..40],
/// claimed_count [40], bump [41].
fn read_v1_fields(agent_state_info: &AccountInfo) -> Result<(Pubkey, u8, u8)> {
    let data = agent_state_info.try_borrow_data()?;
    require!(
        data[..8] == *AgentState::DISCRIMINATOR,
        ShillbotError::AgentStateDiscriminatorMismatch
    );
    let mut agent_bytes = [0u8; 32];
    agent_bytes.copy_from_slice(&data[8..40]);
    Ok((Pubkey::from(agent_bytes), data[40], data[41]))
}

/// Effects + postcondition in one scope. The mutable borrow on `data`
/// lives ONLY inside this fn — calling any method on `agent_state_info`
/// while a borrow is alive (even a read like `data_len()`) panics
/// "RefCell already mutably borrowed" because Anchor's wrapper code
/// re-borrows the AccountInfo on exit.
fn write_current_layout(
    agent_state_info: &AccountInfo,
    old_agent: Pubkey,
    old_claimed_count: u8,
    old_bump: u8,
) -> Result<()> {
    let new_state = AgentState {
        agent: old_agent,
        claimed_count: old_claimed_count,
        total_completed: 0,
        total_earned: 0,
        total_score_sum: 0,
        total_tasks_claimed: 0,
        total_challenges_lost: 0,
        _reserved: [0u8; 8],
        bump: old_bump,
    };
    let mut data = agent_state_info.try_borrow_mut_data()?;
    require!(
        data.len() == CURRENT_AGENT_STATE_SIZE,
        ShillbotError::AgentStateUnsupportedSize
    );
    let mut writer: &mut [u8] = &mut data;
    new_state
        .try_serialize(&mut writer)
        .map_err(|_| error!(ShillbotError::AgentStateDiscriminatorMismatch))?;
    // Round-trip postcondition while we still hold the borrow.
    let _round_trip = AgentState::try_deserialize(&mut &data[..])
        .map_err(|_| error!(ShillbotError::AgentStateDiscriminatorMismatch))?;
    Ok(())
}

#[derive(Accounts)]
pub struct MigrateAgentState<'info> {
    pub agent: Signer<'info>,

    /// CHECK: legacy accounts may be at the 42-byte size, which would prevent
    /// `Account<AgentState>` from opening them. We verify the discriminator,
    /// the agent pubkey stored in the account, and the PDA derivation
    /// explicitly inside the handler — those are the same properties Anchor
    /// would otherwise check.
    #[account(
        mut,
        seeds = [b"agent_state", agent.key().as_ref()],
        bump,
    )]
    pub agent_state: AccountInfo<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_size_constant_matches_v1_layout() {
        // 8 disc + 32 agent + 1 claimed_count + 1 bump
        assert_eq!(LEGACY_AGENT_STATE_SIZE, 42);
    }

    #[test]
    fn current_size_matches_agent_state_space() {
        assert_eq!(CURRENT_AGENT_STATE_SIZE, 90);
    }

    /// Builds a v1 42-byte buffer and walks the same byte-extraction the
    /// handler does. Asserts agent pubkey + claimed_count + bump come back
    /// out at the right offsets. This is the regression guard for "someone
    /// reordered the v1 layout assumptions" — pure unit test, no runtime
    /// context.
    #[test]
    fn v1_bytes_extract_correctly() {
        let agent_pubkey = Pubkey::new_unique();
        let claimed_count: u8 = 5;
        let bump: u8 = 252;

        let mut bytes = Vec::with_capacity(LEGACY_AGENT_STATE_SIZE);
        bytes.extend_from_slice(AgentState::DISCRIMINATOR);
        bytes.extend_from_slice(agent_pubkey.as_ref());
        bytes.push(claimed_count);
        bytes.push(bump);

        assert_eq!(bytes.len(), LEGACY_AGENT_STATE_SIZE);
        assert_eq!(&bytes[..8], AgentState::DISCRIMINATOR);

        let mut agent_bytes = [0u8; 32];
        agent_bytes.copy_from_slice(&bytes[8..40]);
        assert_eq!(Pubkey::from(agent_bytes), agent_pubkey);
        assert_eq!(bytes[40], claimed_count);
        assert_eq!(bytes[41], bump);
    }

    /// Migration round-trip: build the post-migration byte layout and
    /// confirm `try_deserialize` reads back exactly the values the handler
    /// would have written. Mirrors the postcondition `try_deserialize` in
    /// the handler but runs in unit-test context. Specifically asserts
    /// claimed_count is preserved across the migration (the founder
    /// wallet's claimed_count=5 use case from devnet).
    #[test]
    fn post_migration_preserves_claimed_count_and_bump() {
        let agent_pubkey = Pubkey::new_unique();
        let claimed_count: u8 = 5;
        let bump: u8 = 200;

        let new_state = AgentState {
            agent: agent_pubkey,
            claimed_count,
            total_completed: 0,
            total_earned: 0,
            total_score_sum: 0,
            total_tasks_claimed: 0,
            total_challenges_lost: 0,
            _reserved: [0u8; 8],
            bump,
        };

        let mut buf = vec![0u8; CURRENT_AGENT_STATE_SIZE];
        let mut writer: &mut [u8] = &mut buf;
        new_state.try_serialize(&mut writer).unwrap();

        let parsed = AgentState::try_deserialize(&mut &buf[..]).unwrap();
        assert_eq!(parsed.agent, agent_pubkey);
        assert_eq!(parsed.claimed_count, claimed_count);
        assert_eq!(parsed.total_completed, 0);
        assert_eq!(parsed.total_earned, 0);
        assert_eq!(parsed.total_score_sum, 0);
        assert_eq!(parsed.total_tasks_claimed, 0);
        assert_eq!(parsed.total_challenges_lost, 0);
        assert_eq!(parsed._reserved, [0u8; 8]);
        assert_eq!(parsed.bump, bump);
    }
}
