use crate::errors::CoordinationError;
use crate::events::TournamentFinalized;
use crate::state::{GlobalConfig, Tournament};
use anchor_lang::prelude::*;

/// Snapshots the prize pool and stores the merkle root after tournament end.
/// Authority-gated: only `GlobalConfig.authority` can call, to prevent
/// selective inclusion attacks (a malicious caller could post a root that
/// excludes competitors to inflate their own share).
///
/// The merkle root is computed off-chain from all PlayerProfile accounts:
/// each leaf is `keccak256(0x00 || player_wallet || entitlement_le_bytes)`.
/// The full tree is published off-chain; exclusion is publicly detectable.
pub fn finalize_tournament(ctx: Context<FinalizeTournament>, merkle_root: [u8; 32]) -> Result<()> {
    // Checks
    let tournament = &ctx.accounts.tournament;
    require!(
        ctx.accounts.authority.key() == ctx.accounts.global_config.authority,
        CoordinationError::NotAuthority,
    );
    require!(
        Clock::get()?.unix_timestamp > tournament.end_time,
        CoordinationError::TournamentNotEnded,
    );
    require!(!tournament.finalized, CoordinationError::InvalidGameState);

    let prize_snapshot = tournament.prize_lamports;
    let tournament_id = tournament.tournament_id;

    // Effects
    let tournament = &mut ctx.accounts.tournament;
    tournament.finalized = true;
    tournament.prize_snapshot = prize_snapshot;
    tournament.merkle_root = merkle_root;

    // Postconditions
    require!(tournament.finalized, CoordinationError::InvalidGameState);
    require!(
        tournament.prize_snapshot == prize_snapshot,
        CoordinationError::InvalidGameState,
    );

    // Interactions (event emission only)
    emit!(TournamentFinalized {
        tournament_id,
        prize_snapshot,
        merkle_root,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct FinalizeTournament<'info> {
    #[account(
        mut,
        seeds = [b"tournament", tournament.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    pub authority: Signer<'info>,
}
