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

declare_id!("2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P");

#[program]
pub mod coordination_game {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::initialize(ctx)
    }

    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        treasury_split_bps: u16,
    ) -> Result<()> {
        instructions::initialize_config::initialize_config(ctx, treasury_split_bps)
    }

    pub fn update_config(
        ctx: Context<UpdateConfig>,
        treasury_split_bps: u16,
        treasury: Pubkey,
        matchmaker: Pubkey,
        new_authority: Pubkey,
    ) -> Result<()> {
        instructions::update_config::update_config(
            ctx,
            treasury_split_bps,
            treasury,
            matchmaker,
            new_authority,
        )
    }

    pub fn create_tournament(
        ctx: Context<CreateTournament>,
        tournament_id: u64,
        start_time: i64,
        end_time: i64,
    ) -> Result<()> {
        instructions::create_tournament::create_tournament(ctx, tournament_id, start_time, end_time)
    }

    pub fn deposit_stake(ctx: Context<DepositStake>) -> Result<()> {
        instructions::deposit_stake::deposit_stake(ctx)
    }

    pub fn withdraw_stake(ctx: Context<WithdrawStake>) -> Result<()> {
        instructions::withdraw_stake::withdraw_stake(ctx)
    }

    pub fn create_game(
        ctx: Context<CreateGame>,
        stake_lamports: u64,
        matchup_commitment: [u8; 32],
    ) -> Result<()> {
        instructions::create_game::create_game(ctx, stake_lamports, matchup_commitment)
    }

    pub fn join_game(ctx: Context<JoinGame>) -> Result<()> {
        instructions::join_game::join_game(ctx)
    }

    pub fn commit_guess(ctx: Context<CommitGuess>, commitment: [u8; 32]) -> Result<()> {
        instructions::commit_guess::commit_guess(ctx, commitment)
    }

    pub fn reveal_guess(
        ctx: Context<RevealGuess>,
        r: [u8; 32],
        r_matchup: Option<[u8; 32]>,
    ) -> Result<()> {
        instructions::reveal_guess::reveal_guess(ctx, r, r_matchup)
    }

    pub fn resolve_timeout(ctx: Context<ResolveTimeout>) -> Result<()> {
        instructions::resolve_timeout::resolve_timeout(ctx)
    }

    pub fn finalize_tournament(
        ctx: Context<FinalizeTournament>,
        merkle_root: [u8; 32],
    ) -> Result<()> {
        instructions::finalize_tournament::finalize_tournament(ctx, merkle_root)
    }

    pub fn claim_reward(
        ctx: Context<ClaimReward>,
        amount: u64,
        proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        instructions::claim_reward::claim_reward(ctx, amount, proof)
    }

    pub fn close_game(ctx: Context<CloseGame>) -> Result<()> {
        instructions::close_game::close_game(ctx)
    }

    // --- Session key instructions ---

    pub fn create_player_session(ctx: Context<CreatePlayerSession>) -> Result<()> {
        instructions::create_player_session::create_player_session(ctx)
    }

    pub fn close_player_session(ctx: Context<ClosePlayerSession>) -> Result<()> {
        instructions::close_player_session::close_player_session(ctx)
    }

    pub fn close_session_by_delegate(ctx: Context<CloseSessionByDelegate>) -> Result<()> {
        instructions::close_session_by_delegate::close_session_by_delegate(ctx)
    }

    pub fn deposit_stake_session(ctx: Context<DepositStakeSession>) -> Result<()> {
        instructions::deposit_stake_session::deposit_stake_session(ctx)
    }

    pub fn create_game_session(
        ctx: Context<CreateGameSession>,
        stake_lamports: u64,
        matchup_commitment: [u8; 32],
    ) -> Result<()> {
        instructions::create_game_session::create_game_session(
            ctx,
            stake_lamports,
            matchup_commitment,
        )
    }

    pub fn join_game_session(ctx: Context<JoinGameSession>) -> Result<()> {
        instructions::join_game_session::join_game_session(ctx)
    }

    pub fn commit_guess_session(
        ctx: Context<CommitGuessSession>,
        commitment: [u8; 32],
    ) -> Result<()> {
        instructions::commit_guess_session::commit_guess_session(ctx, commitment)
    }

    pub fn reveal_guess_session(
        ctx: Context<RevealGuessSession>,
        r: [u8; 32],
        r_matchup: Option<[u8; 32]>,
    ) -> Result<()> {
        instructions::reveal_guess_session::reveal_guess_session(ctx, r, r_matchup)
    }
}
