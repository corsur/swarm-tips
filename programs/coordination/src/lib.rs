#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod payoff;
pub mod state;

use instructions::*;

declare_id!("2mqqXnhRtqEYUM9ycyL7mLjkCfjjutMXSfYWuXWxEJac");

#[program]
pub mod coordination {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::initialize(ctx)
    }

    pub fn create_tournament(
        ctx: Context<CreateTournament>,
        tournament_id: u64,
        start_time: i64,
        end_time: i64,
    ) -> Result<()> {
        instructions::create_tournament::create_tournament(ctx, tournament_id, start_time, end_time)
    }

    pub fn create_game(ctx: Context<CreateGame>, stake_lamports: u64) -> Result<()> {
        instructions::create_game::create_game(ctx, stake_lamports)
    }

    pub fn join_game(ctx: Context<JoinGame>) -> Result<()> {
        instructions::join_game::join_game(ctx)
    }

    pub fn commit_guess(ctx: Context<CommitGuess>, commitment: [u8; 32]) -> Result<()> {
        instructions::commit_guess::commit_guess(ctx, commitment)
    }

    pub fn reveal_guess(ctx: Context<RevealGuess>, r: [u8; 32]) -> Result<()> {
        instructions::reveal_guess::reveal_guess(ctx, r)
    }

    pub fn resolve_timeout(ctx: Context<ResolveTimeout>) -> Result<()> {
        instructions::resolve_timeout::resolve_timeout(ctx)
    }

    pub fn finalize_tournament(ctx: Context<FinalizeTournament>) -> Result<()> {
        instructions::finalize_tournament::finalize_tournament(ctx)
    }

    pub fn claim_reward(ctx: Context<ClaimReward>) -> Result<()> {
        instructions::claim_reward::claim_reward(ctx)
    }

    pub fn close_game(ctx: Context<CloseGame>) -> Result<()> {
        instructions::close_game::close_game(ctx)
    }
}
