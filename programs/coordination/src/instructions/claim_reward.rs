use anchor_lang::prelude::*;
use crate::errors::CoordinationError;
use crate::events::RewardClaimed;
use crate::state::{PlayerProfile, Tournament, MIN_GAMES_FOR_PAYOUT};

pub fn claim_reward(ctx: Context<ClaimReward>) -> Result<()> {
    let tournament = &ctx.accounts.tournament;
    require!(tournament.finalized, CoordinationError::TournamentNotFinalized);
    require!(tournament.prize_snapshot > 0, CoordinationError::EmptyPrizePool);

    let profile = &ctx.accounts.player_profile;
    require!(!profile.claimed, CoordinationError::AlreadyClaimed);
    require!(
        profile.total_games >= MIN_GAMES_FOR_PAYOUT,
        CoordinationError::BelowMinimumGames,
    );
    require!(
        profile.tournament_id == tournament.tournament_id,
        CoordinationError::ProfileTournamentMismatch,
    );

    // Proportional entitlement: (player_score / total_score) * prize_snapshot
    // Integer math: prize_snapshot * player_score / total_score_snapshot
    let entitlement = if tournament.total_score_snapshot == 0 {
        0u64
    } else {
        tournament
            .prize_snapshot
            .checked_mul(profile.score)
            .ok_or(CoordinationError::ArithmeticOverflow)?
            .checked_div(tournament.total_score_snapshot)
            .ok_or(CoordinationError::ArithmeticOverflow)?
    };

    require!(entitlement > 0, CoordinationError::EmptyPrizePool);

    // Transfer entitlement from tournament PDA to player wallet
    let tournament_info = ctx.accounts.tournament.to_account_info();
    let player_info = ctx.accounts.player.to_account_info();
    **tournament_info.try_borrow_mut_lamports()? = tournament_info
        .lamports()
        .checked_sub(entitlement)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    **player_info.try_borrow_mut_lamports()? = player_info
        .lamports()
        .checked_add(entitlement)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    ctx.accounts.player_profile.claimed = true;

    // Postcondition: claimed flag must be set; prevents double-claim
    require!(ctx.accounts.player_profile.claimed, CoordinationError::InvalidGameState);

    emit!(RewardClaimed {
        tournament_id: tournament.tournament_id,
        player: ctx.accounts.player.key(),
        amount: entitlement,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct ClaimReward<'info> {
    #[account(
        mut,
        seeds = [b"tournament", tournament.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        mut,
        seeds = [
            b"player",
            tournament.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump = player_profile.bump,
        constraint = player_profile.wallet == player.key(),
    )]
    pub player_profile: Account<'info, PlayerProfile>,
    #[account(mut)]
    pub player: Signer<'info>,
}
