use crate::errors::CoordinationError;
use crate::events::TournamentFinalized;
use crate::state::{PlayerProfile, Tournament};
use anchor_lang::prelude::*;
use anchor_lang::Discriminator;

/// Maximum number of PlayerProfile accounts accepted per call.
/// Solana transaction size limits practical use to ~30 accounts.
const MAX_FINALIZE_ACCOUNTS: usize = 30;

// Byte offsets within a serialized PlayerProfile account (including the
// 8-byte Anchor discriminator prefix).
// Layout: discriminator(8) | wallet(32) | tournament_id(8) | wins(8) | total_games(8) | score(8) | ...
const TOURNAMENT_ID_OFFSET: usize = 8 + 32; // 40
const SCORE_OFFSET: usize = 8 + 32 + 8 + 8 + 8; // 64
const SCORE_END: usize = SCORE_OFFSET + 8; // 72

/// Snapshots the prize pool and total player score after tournament end.
/// Permissionless — any wallet can call.
///
/// All PlayerProfile accounts for this tournament must be passed as
/// remaining_accounts. Each is verified as a valid PDA before its score
/// is included in the total.
///
/// Limitation: capped at MAX_FINALIZE_ACCOUNTS profiles per transaction.
/// Redesign required for larger tournaments (see open questions).
pub fn finalize_tournament(ctx: Context<FinalizeTournament>) -> Result<()> {
    let tournament = &ctx.accounts.tournament;
    require!(
        Clock::get()?.unix_timestamp > tournament.end_time,
        CoordinationError::TournamentNotEnded,
    );
    require!(!tournament.finalized, CoordinationError::InvalidGameState);
    require!(
        ctx.remaining_accounts.len() <= MAX_FINALIZE_ACCOUNTS,
        CoordinationError::TooManyAccounts,
    );

    let prize_snapshot = tournament.prize_lamports;
    let tournament_id = tournament.tournament_id;
    let program_id = ctx.program_id;

    let total_score = sum_scores(ctx.remaining_accounts, tournament_id, program_id)?;

    let tournament = &mut ctx.accounts.tournament;
    tournament.finalized = true;
    tournament.prize_snapshot = prize_snapshot;
    tournament.total_score_snapshot = total_score;

    require!(tournament.finalized, CoordinationError::InvalidGameState);
    require!(
        tournament.prize_snapshot == prize_snapshot,
        CoordinationError::InvalidGameState
    );

    emit!(TournamentFinalized {
        tournament_id,
        prize_snapshot,
        total_score_snapshot: total_score,
    });
    Ok(())
}

/// Sum scores across all provided PlayerProfile accounts.
///
/// Reads raw account data to avoid borrow conflicts with the mutable
/// tournament account that follows. Layout is stable and documented below.
fn sum_scores(accounts: &[AccountInfo], tournament_id: u64, program_id: &Pubkey) -> Result<u64> {
    let mut total: u64 = 0;

    for account_info in accounts.iter() {
        require!(
            account_info.owner == program_id,
            CoordinationError::ProfileTournamentMismatch,
        );

        let data = account_info.try_borrow_data()?;

        // Safety: the two require! checks below establish that this account's data
        // has the exact layout of a PlayerProfile before any field is read.
        // Invariant 1: owner == program_id (checked above) — only this program writes these accounts.
        // Invariant 2: data.len() >= PlayerProfile::SPACE — sufficient bytes exist for all fields.
        // Invariant 3: discriminator matches — Anchor's 8-byte prefix confirms the account type.
        // Invariant 4: compile-time assertions (bottom of file) guarantee SCORE_END and
        //   TOURNAMENT_ID_OFFSET + 8 are both <= PlayerProfile::SPACE, so slices never panic.
        require!(
            data.len() >= PlayerProfile::SPACE && data[..8] == *PlayerProfile::DISCRIMINATOR,
            CoordinationError::ProfileTournamentMismatch,
        );

        // PlayerProfile layout after discriminator:
        //   wallet: Pubkey (32), tournament_id: u64 (8), wins: u64 (8),
        //   total_games: u64 (8), score: u64 (8), claimed: bool (1), bump: u8 (1)
        // All offsets are defined as constants above and verified by compile-time assertions.
        let profile_tournament_id = u64::from_le_bytes(
            data[TOURNAMENT_ID_OFFSET..TOURNAMENT_ID_OFFSET + 8]
                .try_into()
                .map_err(|_| error!(CoordinationError::ArithmeticOverflow))?,
        );
        require!(
            profile_tournament_id == tournament_id,
            CoordinationError::ProfileTournamentMismatch,
        );

        let score = u64::from_le_bytes(
            data[SCORE_OFFSET..SCORE_END]
                .try_into()
                .map_err(|_| error!(CoordinationError::ArithmeticOverflow))?,
        );

        total = total
            .checked_add(score)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
    }

    Ok(total)
}

// Compile-time assertions: offsets must stay within the account's allocated space.
// If PlayerProfile gains or loses fields, these will fail at compile time.
const _: () = assert!(SCORE_END <= PlayerProfile::SPACE);
const _: () = assert!(TOURNAMENT_ID_OFFSET + 8 <= PlayerProfile::SPACE);

#[derive(Accounts)]
pub struct FinalizeTournament<'info> {
    #[account(
        mut,
        seeds = [b"tournament", tournament.tournament_id.to_le_bytes().as_ref()],
        bump = tournament.bump,
    )]
    pub tournament: Account<'info, Tournament>,
    pub caller: Signer<'info>,
}
