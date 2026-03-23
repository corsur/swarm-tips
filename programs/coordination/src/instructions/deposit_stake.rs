use crate::errors::CoordinationError;
use crate::events::StakeDeposited;
use crate::state::{StakeEscrow, Tournament, FIXED_STAKE_LAMPORTS};
use anchor_lang::prelude::*;
use anchor_lang::system_program;

/// Deposit the fixed stake into a per-player escrow PDA.
///
/// Players must call this before joining the matchmaking queue. The escrow
/// proves they have committed real SOL and are ready to play. The escrow is
/// consumed when a game is created or joined; if the player leaves the queue
/// without playing, they call `withdraw_stake` to reclaim their deposit.
pub fn deposit_stake(ctx: Context<DepositStake>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );

    let escrow = &mut ctx.accounts.escrow;

    // Idempotent: if the escrow already has an unconsumed funded deposit, no-op.
    // This lets callers always call deposit_stake before each game without
    // worrying about whether a prior deposit is still active.
    if !escrow.consumed && escrow.amount > 0 {
        require!(
            escrow.player == ctx.accounts.player.key(),
            CoordinationError::InvalidGameState,
        );
        require!(
            escrow.amount == FIXED_STAKE_LAMPORTS,
            CoordinationError::StakeMismatch,
        );
        msg!("deposit_stake: escrow already active, no-op");
        return Ok(());
    }
    escrow.player = ctx.accounts.player.key();
    escrow.tournament_id = ctx.accounts.tournament.tournament_id;
    escrow.amount = FIXED_STAKE_LAMPORTS;
    escrow.consumed = false;
    escrow.bump = ctx.bumps.escrow;

    // Postconditions
    require!(
        escrow.player == ctx.accounts.player.key(),
        CoordinationError::InvalidGameState,
    );
    require!(
        escrow.amount == FIXED_STAKE_LAMPORTS,
        CoordinationError::StakeMismatch,
    );

    // Transfer stake from player to escrow PDA
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.player.to_account_info(),
                to: ctx.accounts.escrow.to_account_info(),
            },
        ),
        FIXED_STAKE_LAMPORTS,
    )?;

    emit!(StakeDeposited {
        player: ctx.accounts.player.key(),
        tournament_id: ctx.accounts.tournament.tournament_id,
        amount: FIXED_STAKE_LAMPORTS,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct DepositStake<'info> {
    #[account(
        init_if_needed,
        payer = player,
        space = StakeEscrow::SPACE,
        seeds = [
            b"escrow",
            tournament.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump,
    )]
    pub escrow: Account<'info, StakeEscrow>,
    pub tournament: Account<'info, Tournament>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}
