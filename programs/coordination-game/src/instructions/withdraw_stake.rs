use crate::errors::CoordinationError;
use crate::events::StakeWithdrawn;
use crate::state::StakeEscrow;
use anchor_lang::prelude::*;

/// Allow a player to reclaim their unconsumed escrow deposit.
///
/// The escrow must not have been consumed by a game instruction. Closing the
/// account returns rent + remaining lamports to the player.
pub fn withdraw_stake(ctx: Context<WithdrawStake>) -> Result<()> {
    let escrow = &ctx.accounts.escrow;

    // Checks
    require!(
        escrow.player == ctx.accounts.player.key(),
        CoordinationError::InvalidGameState,
    );
    require!(!escrow.consumed, CoordinationError::EscrowAlreadyConsumed,);

    // Capture values before the account is closed by Anchor
    let tournament_id = escrow.tournament_id;
    let amount = escrow.amount;

    // Postcondition: escrow holds a positive deposit
    require!(amount > 0, CoordinationError::StakeMismatch);

    // Effects + Interactions: Anchor `close = player` constraint handles
    // closing the account and transferring all lamports to the player.

    emit!(StakeWithdrawn {
        wallet: ctx.accounts.player.key(),
        tournament_id,
        amount,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawStake<'info> {
    #[account(
        mut,
        seeds = [
            b"escrow",
            escrow.tournament_id.to_le_bytes().as_ref(),
            player.key().as_ref(),
        ],
        bump = escrow.bump,
        has_one = player @ CoordinationError::InvalidGameState,
        close = player,
    )]
    pub escrow: Account<'info, StakeEscrow>,
    #[account(mut)]
    pub player: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::FIXED_STAKE_LAMPORTS;

    #[test]
    fn consumed_escrow_is_rejected_by_validate() {
        let pk = Pubkey::new_unique();
        let escrow = StakeEscrow {
            player: pk,
            tournament_id: 1,
            amount: FIXED_STAKE_LAMPORTS,
            consumed: true,
            bump: 255,
        };
        assert!(!escrow.validate_for_game(&pk, 1));
    }

    #[test]
    fn unconsumed_escrow_passes_validate() {
        let pk = Pubkey::new_unique();
        let escrow = StakeEscrow {
            player: pk,
            tournament_id: 1,
            amount: FIXED_STAKE_LAMPORTS,
            consumed: false,
            bump: 255,
        };
        assert!(escrow.validate_for_game(&pk, 1));
    }
}
