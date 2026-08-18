#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod scoring;
pub mod state;
pub mod transfers;

use instructions::*;

declare_id!("2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi");

// --- Constants ---
// Default values used in `initialize` for new GlobalState fields.
pub const DEFAULT_CHALLENGE_WINDOW_SECONDS: i64 = 86_400; // 24 hours
pub const DEFAULT_STALENESS_WINDOW_SECONDS: i64 = 86_400; // 1 day tolerance for oracle attestation
pub const DEFAULT_ATTESTATION_DELAY_SECONDS: i64 = 604_800; // 7 days — expected oracle attestation delay
pub const DEFAULT_VERIFICATION_TIMEOUT_SECONDS: i64 = 1_209_600; // 14 days
pub const MAX_EMERGENCY_RETURN_ACCOUNTS: usize = 20;
pub const DEFAULT_MAX_CONCURRENT_CLAIMS: u8 = 5;
pub const MIN_CLAIM_BUFFER_SECONDS: i64 = 14_400; // 4 hours
pub const MIN_CHALLENGE_BOND_MULTIPLIER: u8 = 2; // 2x full task price
pub const MAX_CHALLENGE_BOND_MULTIPLIER: u8 = 10; // 10x full task price
pub const DEFAULT_CHALLENGE_BOND_MULTIPLIER: u8 = 2;
pub const DEFAULT_BOND_SLASH_TREASURY_BPS: u16 = 5_000; // 50%
pub const MAX_CONTENT_ID_LENGTH: usize = 256;
// Dispute-resolution liveness window (2026-07-07): bounds for the
// permissionless resolve_challenge_default arming delay. 0 disables.
pub const MIN_DISPUTE_RESOLUTION_WINDOW_SECONDS: i64 = 3_600; // 1 hour
pub const MAX_DISPUTE_RESOLUTION_WINDOW_SECONDS: i64 = 2_592_000; // 30 days
pub const DEFAULT_DISPUTE_RESOLUTION_WINDOW_SECONDS: i64 = 604_800; // 7 days

#[program]
pub mod shillbot {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        protocol_fee_bps: u16,
        quality_threshold: u64,
        starting_counter: u64,
        switchboard_feed: Pubkey,
    ) -> Result<()> {
        instructions::initialize::initialize(
            ctx,
            protocol_fee_bps,
            quality_threshold,
            starting_counter,
            switchboard_feed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_params(
        ctx: Context<UpdateParams>,
        protocol_fee_bps: u16,
        quality_threshold: u64,
        challenge_window_seconds: i64,
        verification_timeout_seconds: i64,
        attestation_delay_seconds: i64,
        staleness_window_seconds: i64,
        max_concurrent_claims: u8,
        challenge_bond_multiplier: u8,
        bond_slash_treasury_bps: u16,
        paused: bool,
        paused_platforms: u16,
        rate_limit_window_seconds: i64,
        max_tasks_per_rate_window: u32,
        dispute_resolution_window_seconds: i64,
    ) -> Result<()> {
        instructions::update_params::update_params(
            ctx,
            protocol_fee_bps,
            quality_threshold,
            challenge_window_seconds,
            verification_timeout_seconds,
            attestation_delay_seconds,
            staleness_window_seconds,
            max_concurrent_claims,
            challenge_bond_multiplier,
            bond_slash_treasury_bps,
            paused,
            paused_platforms,
            rate_limit_window_seconds,
            max_tasks_per_rate_window,
            dispute_resolution_window_seconds,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_task(
        ctx: Context<CreateTask>,
        nonce: u64,
        escrow_lamports: u64,
        content_hash: [u8; 32],
        deadline: i64,
        submit_margin: i64,
        claim_buffer: i64,
        platform: u8,
        attestation_delay_override: u32,
        challenge_window_override: u32,
        verification_timeout_override: u32,
        requires_approval: bool,
        verification_kind: u8,
    ) -> Result<()> {
        instructions::create_task::create_task(
            ctx,
            nonce,
            escrow_lamports,
            content_hash,
            deadline,
            submit_margin,
            claim_buffer,
            platform,
            attestation_delay_override,
            challenge_window_override,
            verification_timeout_override,
            requires_approval,
            verification_kind,
        )
    }

    pub fn claim_task(ctx: Context<ClaimTask>) -> Result<()> {
        instructions::claim_task::claim_task(ctx)
    }

    pub fn close_agent_state(ctx: Context<CloseAgentState>) -> Result<()> {
        instructions::close_agent_state::close_agent_state(ctx)
    }

    pub fn migrate_agent_state(ctx: Context<MigrateAgentState>) -> Result<()> {
        instructions::migrate_agent_state::migrate_agent_state(ctx)
    }

    /// One-shot realloc of legacy 315-byte Task accounts to the 347-byte
    /// payout_to layout (C2). Permissionless + idempotent.
    pub fn migrate_task(ctx: Context<MigrateTask>) -> Result<()> {
        instructions::migrate_task::migrate_task(ctx)
    }

    /// Agent routes this task's finalize payment to `payout_to` (e.g. an
    /// extension-credit splitter vault). Reputation stays on `task.agent`.
    pub fn set_payout_to(ctx: Context<SetPayoutTo>, payout_to: Pubkey) -> Result<()> {
        instructions::set_payout_to::set_payout_to(ctx, payout_to)
    }

    pub fn submit_work(ctx: Context<SubmitWork>, content_id: Vec<u8>) -> Result<()> {
        instructions::submit_work::submit_work(ctx, content_id)
    }

    pub fn approve_task(ctx: Context<ApproveTask>) -> Result<()> {
        instructions::approve_task::approve_task(ctx)
    }

    pub fn verify_task(
        ctx: Context<VerifyTask>,
        composite_score: u64,
        verification_hash: [u8; 32],
    ) -> Result<()> {
        instructions::verify_task::verify_task(ctx, composite_score, verification_hash)
    }

    /// Attester-signed verification for `verification_kind = 1` tasks —
    /// the `oracle_authority` signs the transaction; `verification_hash`
    /// is the chain-core `AttestationCert` digest.
    pub fn verify_task_attested(
        ctx: Context<VerifyTaskAttested>,
        score: u64,
        verification_hash: [u8; 32],
    ) -> Result<()> {
        instructions::verify_task_attested::verify_task_attested(ctx, score, verification_hash)
    }

    pub fn finalize_task(ctx: Context<FinalizeTask>) -> Result<()> {
        instructions::finalize_task::finalize_task(ctx)
    }

    pub fn challenge_task(ctx: Context<ChallengeTask>) -> Result<()> {
        instructions::challenge_task::challenge_task(ctx)
    }

    pub fn resolve_challenge(ctx: Context<ResolveChallenge>, challenger_won: bool) -> Result<()> {
        instructions::resolve_challenge::resolve_challenge(ctx, challenger_won)
    }

    /// Permissionless default resolution of a Disputed task once the
    /// dispute-resolution window elapses without authority adjudication:
    /// pinned payment executes, bond returns un-slashed.
    pub fn resolve_challenge_default(ctx: Context<ResolveChallengeDefault>) -> Result<()> {
        instructions::resolve_challenge_default::resolve_challenge_default(ctx)
    }

    pub fn expire_task(ctx: Context<ExpireTask>) -> Result<()> {
        instructions::expire_task::expire_task(ctx)
    }

    pub fn emergency_return(ctx: Context<EmergencyReturnAccounts>) -> Result<()> {
        instructions::emergency_return::emergency_return(ctx)
    }

    pub fn create_session(
        ctx: Context<CreateSession>,
        allowed_instructions: u8,
        duration_seconds: i64,
    ) -> Result<()> {
        instructions::create_session::create_session(ctx, allowed_instructions, duration_seconds)
    }

    pub fn revoke_session(ctx: Context<RevokeSession>) -> Result<()> {
        instructions::revoke_session::revoke_session(ctx)
    }

    pub fn claim_task_session(ctx: Context<ClaimTaskSession>) -> Result<()> {
        instructions::claim_task_session::claim_task_session(ctx)
    }

    pub fn submit_work_session(ctx: Context<SubmitWorkSession>, content_id: Vec<u8>) -> Result<()> {
        instructions::submit_work_session::submit_work_session(ctx, content_id)
    }

    pub fn register_identity(
        ctx: Context<RegisterIdentity>,
        platform: u8,
        identity_hash: [u8; 32],
    ) -> Result<()> {
        instructions::register_identity::register_identity(ctx, platform, identity_hash)
    }

    pub fn revoke_identity(ctx: Context<RevokeIdentity>) -> Result<()> {
        instructions::revoke_identity::revoke_identity(ctx)
    }

    pub fn transfer_authority(
        ctx: Context<TransferAuthority>,
        new_authority: Pubkey,
    ) -> Result<()> {
        instructions::transfer_authority::transfer_authority(ctx, new_authority)
    }

    pub fn update_treasury(ctx: Context<UpdateTreasury>, new_treasury: Pubkey) -> Result<()> {
        instructions::update_treasury::update_treasury(ctx, new_treasury)
    }

    pub fn update_oracle_authority(
        ctx: Context<UpdateOracleAuthority>,
        new_oracle_authority: Pubkey,
    ) -> Result<()> {
        instructions::update_oracle_authority::update_oracle_authority(ctx, new_oracle_authority)
    }
}
