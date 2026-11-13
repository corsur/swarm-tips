use crate::errors::CoordinationError;
use crate::events::TournamentCreated;
use crate::state::{GlobalConfig, Tournament};
use anchor_lang::prelude::*;

pub fn create_tournament(
    ctx: Context<CreateTournament>,
    tournament_id: u64,
    start_time: i64,
    end_time: i64,
) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.authority.key(),
        ctx.accounts.global_config.authority,
        CoordinationError::NotAuthority
    );
    let now = Clock::get()?.unix_timestamp;
    require!(
        end_time > start_time,
        CoordinationError::InvalidTournamentTimes
    );
    require!(end_time > now, CoordinationError::TournamentEndsInPast);

    let t = &mut ctx.accounts.tournament;
    t.tournament_id = tournament_id;
    t.authority = ctx.accounts.authority.key();
    t.start_time = start_time;
    t.end_time = end_time;
    t.prize_lamports = 0;
    t.game_count = 0;
    t.finalized = false;
    t.prize_snapshot = 0;
    t.merkle_root = [0u8; 32];
    t.bump = ctx.bumps.tournament;

    // Postconditions: verify tournament was initialized correctly
    require!(!t.finalized, CoordinationError::InvalidGameState);
    require!(
        t.end_time > t.start_time,
        CoordinationError::InvalidTournamentTimes
    );

    emit!(TournamentCreated {
        tournament_id,
        start_time,
        end_time
    });
    Ok(())
}

#[derive(Accounts)]
#[instruction(tournament_id: u64)]
pub struct CreateTournament<'info> {
    #[account(
        init,
        payer = authority,
        space = Tournament::SPACE,
        seeds = [b"tournament", tournament_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub tournament: Account<'info, Tournament>,
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
        constraint = global_config.authority == authority.key() @ CoordinationError::NotAuthority,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}
