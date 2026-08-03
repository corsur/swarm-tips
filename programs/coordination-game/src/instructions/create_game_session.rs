use crate::events::GameCreated;
use crate::instructions::create_game::{commit_creation_state, validate_create_inputs};
use crate::instructions::session_utils::validate_session_authority;
use crate::instructions::utils::transfer_lamports;
use crate::state::{
    Game, GameCounter, GlobalConfig, PlayerProfile, SessionAuthority, StakeEscrow, Tournament,
};
use anchor_lang::prelude::*;

/// Session-delegated variant of `create_game`. The session key signs instead
/// of the player wallet; the matchmaker wallet is verified against
/// GlobalConfig but does not need to sign (session authority proves
/// matchmaker delegation). Helpers shared with `create_game.rs`.
pub fn create_game_session(
    ctx: Context<CreateGameSession>,
    stake_lamports: u64,
    matchup_commitment: [u8; 32],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let player_key = ctx.accounts.player.key();
    let tournament_id = ctx.accounts.tournament.tournament_id;
    // Checks
    validate_session_authority(
        &ctx.accounts.session_authority,
        &ctx.accounts.matchmaker_wallet.key(),
        &ctx.accounts.session_signer.key(),
    )?;
    validate_create_inputs(
        stake_lamports,
        matchup_commitment,
        ctx.accounts.matchmaker_wallet.key(),
        ctx.accounts.global_config.matchmaker,
        &ctx.accounts.tournament,
        &ctx.accounts.escrow,
        &player_key,
        tournament_id,
        now,
        ctx.accounts.global_config.stake_lamports,
    )?;
    // Effects
    let game_id = commit_creation_state(
        &mut ctx.accounts.game_counter,
        &mut ctx.accounts.game,
        &mut ctx.accounts.player_profile,
        &mut ctx.accounts.escrow,
        tournament_id,
        player_key,
        stake_lamports,
        matchup_commitment,
        now,
        ctx.bumps.game,
        ctx.bumps.player_profile,
    )?;
    // Interactions
    transfer_lamports(
        &ctx.accounts.escrow.to_account_info(),
        &ctx.accounts.game.to_account_info(),
        stake_lamports,
    )?;
    emit!(GameCreated {
        game_id,
        tournament_id,
        player_one: player_key,
        stake_lamports,
    });
    Ok(())
}

#[derive(Accounts)]
#[instruction(stake_lamports: u64, matchup_commitment: [u8; 32])]
pub struct CreateGameSession<'info> {
    #[account(
        init,
        payer = session_signer,
        space = Game::SPACE,
        seeds = [b"game", game_counter.count.to_le_bytes().as_ref()],
        bump,
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [b"game_counter"],
        bump = game_counter.bump,
    )]
    pub game_counter: Account<'info, GameCounter>,
    #[account(
        init_if_needed,
        payer = session_signer,
        space = PlayerProfile::SPACE,
        seeds = [
            b"player",
            tournament.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump,
    )]
    pub player_profile: Account<'info, PlayerProfile>,
    #[account(
        mut,
        seeds = [
            b"escrow",
            tournament.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, StakeEscrow>,
    #[account(
        seeds = [b"tournament", tournament.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    /// CHECK: The player wallet. Not a signer — the session key signs instead.
    /// Verified via escrow seeds and session_authority.
    pub player: UncheckedAccount<'info>,
    /// CHECK: The matchmaker wallet. Verified against global_config.matchmaker.
    pub matchmaker_wallet: UncheckedAccount<'info>,
    #[account(
        seeds = [
            b"game_session",
            matchmaker_wallet.key().as_ref(),
            session_signer.key().as_ref(),
        ],
        bump = session_authority.bump,
    )]
    pub session_authority: Account<'info, SessionAuthority>,
    #[account(mut)]
    pub session_signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
