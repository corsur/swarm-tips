use crate::errors::CoordinationError;
use crate::events::StakeDeposited;
use crate::state::{GlobalConfig, StakeEscrow, Tournament};
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
    // The live configured stake. Reading it here (rather than a constant) is
    // what lets a re-peg be an instruction instead of a program upgrade.
    let stake = ctx.accounts.global_config.stake_lamports;
    require!(
        ctx.accounts.tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );

    let escrow = &mut ctx.accounts.escrow;

    // Idempotent: if the escrow already has an unconsumed funded deposit at the
    // correct amount, no-op. If the amount doesn't match (e.g., stake was changed
    // via program upgrade), fall through to re-deposit at the new amount.
    if !escrow.consumed && escrow.amount > 0 {
        require!(
            escrow.player == ctx.accounts.player.key(),
            CoordinationError::InvalidGameState,
        );
        if escrow.amount == stake {
            msg!("deposit_stake: escrow already active, no-op");
            return Ok(());
        }
        // Stake amount changed — fall through to re-deposit at the new amount.
        // The old lamports remain in the account; the new transfer tops it up.
        msg!("deposit_stake: stake amount changed, re-depositing");
    }
    escrow.player = ctx.accounts.player.key();
    escrow.tournament_id = ctx.accounts.tournament.tournament_id;
    escrow.amount = stake;
    escrow.consumed = false;
    escrow.bump = ctx.bumps.escrow;

    // Postconditions
    require!(
        escrow.player == ctx.accounts.player.key(),
        CoordinationError::InvalidGameState,
    );
    require!(escrow.amount == stake, CoordinationError::StakeMismatch,);

    // Transfer stake from player to escrow PDA
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.player.to_account_info(),
                to: ctx.accounts.escrow.to_account_info(),
            },
        ),
        stake,
    )?;

    emit!(StakeDeposited {
        player: ctx.accounts.player.key(),
        tournament_id: ctx.accounts.tournament.tournament_id,
        amount: stake,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct DepositStake<'info> {
    /// Source of the live stake. REQUIRED (not optional): an escrow funded at a
    /// superseded amount must never validate for a game, or one player could
    /// enter having staked less than the other.
    #[account(seeds = [b"global_config"], bump = global_config.bump)]
    pub global_config: Account<'info, GlobalConfig>,
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
