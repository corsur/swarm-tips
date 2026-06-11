#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

use anchor_lang::prelude::*;

pub mod cert;
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

    pub fn sweep_unclaimed_to_next(ctx: Context<SweepUnclaimedToNext>) -> Result<()> {
        instructions::sweep_unclaimed::sweep_unclaimed_to_next(ctx)
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

    // ----- Cross-chain (Solana↔EVM) match instructions -----

    pub fn initialize_xpool(
        ctx: Context<InitializeXPool>,
        operator: Pubkey,
        operator_signer: [u8; 20],
        max_tranche_lamports: u64,
        max_claim_window_secs: u32,
        skew_margin_secs: u32,
    ) -> Result<()> {
        instructions::xchain::initialize_xpool(
            ctx,
            operator,
            operator_signer,
            max_tranche_lamports,
            max_claim_window_secs,
            skew_margin_secs,
        )
    }

    pub fn xpool_deposit(ctx: Context<XPoolDeposit>, amount: u64) -> Result<()> {
        instructions::xchain::xpool_deposit(ctx, amount)
    }

    pub fn xpool_withdraw(ctx: Context<XPoolWithdraw>, amount: u64) -> Result<()> {
        instructions::xchain::xpool_withdraw(ctx, amount)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_xmatch(
        ctx: Context<CreateXMatch>,
        match_id: [u8; 32],
        tournament_id: u64,
        player_is_p1: bool,
        session_key: [u8; 20],
        counter_session_key: [u8; 20],
        stake_lamports: u64,
        fund_deadline: i64,
        match_deadline: i64,
    ) -> Result<()> {
        instructions::xchain::create_xmatch(
            ctx,
            match_id,
            tournament_id,
            player_is_p1,
            session_key,
            counter_session_key,
            stake_lamports,
            fund_deadline,
            match_deadline,
        )
    }

    pub fn lock_xtranche(
        ctx: Context<LockXTranche>,
        match_id: [u8; 32],
        tranche_lamports: u64,
    ) -> Result<()> {
        instructions::xchain::lock_xtranche(ctx, match_id, tranche_lamports)
    }

    pub fn settle_xmatch(
        ctx: Context<SettleXMatch>,
        cert: cert::MatchLiveCertArg,
        outcome: cert::OutcomeCertArg,
        live_sigs: [[u8; 65]; 3],
        oc_sigs: [[u8; 65]; 3],
    ) -> Result<()> {
        instructions::xchain::settle_xmatch(ctx, cert, outcome, live_sigs, oc_sigs)
    }

    pub fn open_xclaim(
        ctx: Context<OpenXClaim>,
        cert: cert::MatchLiveCertArg,
        cp: cert::CheckpointArg,
        live_sigs: [[u8; 65]; 3],
        cp_sigs: [[u8; 65]; 2],
    ) -> Result<()> {
        instructions::xchain::open_xclaim(ctx, cert, cp, live_sigs, cp_sigs)
    }

    pub fn supersede_xclaim(
        ctx: Context<SupersedeXClaim>,
        cert: cert::MatchLiveCertArg,
        cp: cert::CheckpointArg,
        cp_sigs: [[u8; 65]; 2],
    ) -> Result<()> {
        instructions::xchain::supersede_xclaim(ctx, cert, cp, cp_sigs)
    }

    pub fn submit_equivocation_proof(
        ctx: Context<SubmitEquivocation>,
        cert: cert::MatchLiveCertArg,
        cp_a: cert::CheckpointArg,
        cp_b: cert::CheckpointArg,
        sig_a: [u8; 65],
        sig_b: [u8; 65],
    ) -> Result<()> {
        instructions::xchain::submit_equivocation_proof(ctx, cert, cp_a, cp_b, sig_a, sig_b)
    }

    pub fn settle_xclaim(ctx: Context<SettleXClaim>) -> Result<()> {
        instructions::xchain::settle_xclaim(ctx)
    }

    pub fn refund_xmatch_nocert(ctx: Context<RefundXMatch>) -> Result<()> {
        instructions::xchain::refund_xmatch_nocert(ctx)
    }

    pub fn refund_xmatch_timeout(ctx: Context<RefundXMatchTimeout>) -> Result<()> {
        instructions::xchain::refund_xmatch_timeout(ctx)
    }

    pub fn close_xmatch(ctx: Context<CloseXMatch>) -> Result<()> {
        instructions::xchain::close_xmatch(ctx)
    }
}
