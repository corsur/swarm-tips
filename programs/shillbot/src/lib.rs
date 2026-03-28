#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod scoring;
pub mod state;

use instructions::*;

declare_id!("2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi");

// --- Constants ---
pub const CHALLENGE_WINDOW_SECONDS: i64 = 86_400; // 24 hours
pub const VERIFICATION_TIMEOUT_SECONDS: i64 = 1_209_600; // 14 days
pub const MAX_CONCURRENT_CLAIMS: u8 = 5;
pub const MIN_CLAIM_BUFFER_SECONDS: i64 = 14_400; // 4 hours
pub const FREE_CHALLENGE_PERCENT: u16 = 20; // 20% of campaign tasks
pub const MIN_CHALLENGE_BOND_MULTIPLIER: u8 = 2; // 2x full task price
pub const MAX_CHALLENGE_BOND_MULTIPLIER: u8 = 5; // 5x full task price

#[program]
pub mod shillbot {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        protocol_fee_bps: u16,
        quality_threshold: u64,
    ) -> Result<()> {
        instructions::initialize::initialize(ctx, protocol_fee_bps, quality_threshold)
    }

    pub fn update_params(
        ctx: Context<UpdateParams>,
        protocol_fee_bps: u16,
        quality_threshold: u64,
    ) -> Result<()> {
        instructions::update_params::update_params(ctx, protocol_fee_bps, quality_threshold)
    }

    pub fn create_task(
        ctx: Context<CreateTask>,
        escrow_lamports: u64,
        content_hash: [u8; 32],
        deadline: i64,
        submit_margin: i64,
        claim_buffer: i64,
    ) -> Result<()> {
        instructions::create_task::create_task(
            ctx,
            escrow_lamports,
            content_hash,
            deadline,
            submit_margin,
            claim_buffer,
        )
    }

    pub fn claim_task(ctx: Context<ClaimTask>) -> Result<()> {
        instructions::claim_task::claim_task(ctx)
    }

    pub fn submit_work(ctx: Context<SubmitWork>, video_id: Vec<u8>) -> Result<()> {
        instructions::submit_work::submit_work(ctx, video_id)
    }

    pub fn verify_task(ctx: Context<VerifyTask>, composite_score: u64) -> Result<()> {
        instructions::verify_task::verify_task(ctx, composite_score)
    }

    pub fn finalize_task(ctx: Context<FinalizeTask>) -> Result<()> {
        instructions::finalize_task::finalize_task(ctx)
    }

    pub fn challenge_task(ctx: Context<ChallengeTask>, total_campaign_tasks: u16) -> Result<()> {
        instructions::challenge_task::challenge_task(ctx, total_campaign_tasks)
    }

    pub fn resolve_challenge(ctx: Context<ResolveChallenge>, challenger_won: bool) -> Result<()> {
        instructions::resolve_challenge::resolve_challenge(ctx, challenger_won)
    }

    pub fn expire_task(ctx: Context<ExpireTask>) -> Result<()> {
        instructions::expire_task::expire_task(ctx)
    }

    pub fn emergency_return(ctx: Context<EmergencyReturnAccounts>) -> Result<()> {
        instructions::emergency_return::emergency_return(ctx)
    }

    pub fn create_session(ctx: Context<CreateSession>, allowed_instructions: u8) -> Result<()> {
        instructions::create_session::create_session(ctx, allowed_instructions)
    }

    pub fn revoke_session(ctx: Context<RevokeSession>) -> Result<()> {
        instructions::revoke_session::revoke_session(ctx)
    }
}
