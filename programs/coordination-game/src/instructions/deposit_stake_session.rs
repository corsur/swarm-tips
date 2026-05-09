use crate::errors::CoordinationError;
use crate::events::StakeDeposited;
use crate::instructions::session_utils::validate_session_authority;
use crate::state::{SessionAuthority, StakeEscrow, Tournament, FIXED_STAKE_LAMPORTS};
use anchor_lang::prelude::*;

/// Session-delegated variant of `deposit_stake`. The session key signs the
/// transaction instead of the player wallet. The session authority PDA's
/// lamports fund the stake transfer (the player pre-funded the session PDA
/// at creation time).
pub fn deposit_stake_session(ctx: Context<DepositStakeSession>) -> Result<()> {
    validate_session_authority(
        &ctx.accounts.session_authority,
        &ctx.accounts.player.key(),
        &ctx.accounts.session_signer.key(),
    )?;
    let now = Clock::get()?.unix_timestamp;
    require!(
        ctx.accounts.tournament.is_active(now),
        CoordinationError::OutsideTournamentWindow,
    );
    let player_key = ctx.accounts.player.key();
    let tournament_id = ctx.accounts.tournament.tournament_id;
    // Idempotent: if escrow is already valid for this game, no-op (avoids
    // re-transferring lamports). Otherwise (re-)init the escrow fields.
    if escrow_already_active(&ctx.accounts.escrow, &player_key)? {
        msg!("deposit_stake_session: escrow already active, no-op");
        return Ok(());
    }
    init_escrow_fields(
        &mut ctx.accounts.escrow,
        player_key,
        tournament_id,
        ctx.bumps.escrow,
    )?;
    // Interactions: transfer stake from session signer to escrow PDA. The
    // signer was pre-funded by the player at session-setup time.
    anchor_lang::system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.session_signer.to_account_info(),
                to: ctx.accounts.escrow.to_account_info(),
            },
        ),
        FIXED_STAKE_LAMPORTS,
    )?;
    emit!(StakeDeposited {
        player: player_key,
        tournament_id,
        amount: FIXED_STAKE_LAMPORTS,
    });
    Ok(())
}

/// Checks if the escrow is already an unconsumed deposit for this player
/// at the current FIXED_STAKE_LAMPORTS. Returns true if the handler should
/// no-op (don't re-transfer); false if re-init + transfer is required.
/// Errors only if the escrow is funded but bound to a different player.
fn escrow_already_active(escrow: &StakeEscrow, player_key: &Pubkey) -> Result<bool> {
    if escrow.consumed || escrow.amount == 0 {
        return Ok(false);
    }
    require!(
        escrow.player == *player_key,
        CoordinationError::InvalidGameState,
    );
    if escrow.amount == FIXED_STAKE_LAMPORTS {
        return Ok(true);
    }
    msg!("deposit_stake_session: stake amount changed, re-depositing");
    Ok(false)
}

/// Pure effect: write the escrow fields for a fresh (or re-init) deposit.
fn init_escrow_fields(
    escrow: &mut StakeEscrow,
    player_key: Pubkey,
    tournament_id: u64,
    bump: u8,
) -> Result<()> {
    escrow.player = player_key;
    escrow.tournament_id = tournament_id;
    escrow.amount = FIXED_STAKE_LAMPORTS;
    escrow.consumed = false;
    escrow.bump = bump;
    require!(
        escrow.player == player_key,
        CoordinationError::InvalidGameState,
    );
    require!(
        escrow.amount == FIXED_STAKE_LAMPORTS,
        CoordinationError::StakeMismatch,
    );
    Ok(())
}

#[derive(Accounts)]
pub struct DepositStakeSession<'info> {
    #[account(
        init_if_needed,
        payer = session_signer,
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
    /// CHECK: The player wallet. Not a signer — the session key signs instead.
    /// Verified against session_authority.player in the handler.
    pub player: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [
            b"game_session",
            player.key().as_ref(),
            session_signer.key().as_ref(),
        ],
        bump = session_authority.bump,
    )]
    pub session_authority: Account<'info, SessionAuthority>,
    #[account(mut)]
    pub session_signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
