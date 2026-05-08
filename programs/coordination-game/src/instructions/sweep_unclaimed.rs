use crate::errors::CoordinationError;
use crate::events::UnclaimedSwept;
use crate::state::{GlobalConfig, Tournament};
use anchor_lang::prelude::*;

/// Grace period after a finalized tournament's `end_time` before unclaimed
/// lamports may be swept into the next tournament's prize pool. 90 days —
/// long enough that any legitimately-eligible player has had a fair shot
/// to claim. After this window, anything still parked in the source PDA
/// is rolled forward.
pub const UNCLAIMED_GRACE_SECS: i64 = 90 * 24 * 60 * 60;

/// Authority-gated sweep: moves unclaimed lamports from a finalized source
/// tournament into a still-active destination tournament's prize pool.
///
/// The source PDA stays open (rent floor preserved) so its merkle_root and
/// historical fields remain readable on-chain. After the sweep, any player
/// who hadn't called claim_reward by T+grace will find their `claim_reward`
/// fails with InsufficientLamports — that's the social contract.
///
/// State after the sweep:
///   src.lamports() == rent_minimum (account stays open, but empty of pool)
///   dest.lamports() += sweepable
///   dest.prize_lamports += sweepable
pub fn sweep_unclaimed_to_next(ctx: Context<SweepUnclaimedToNext>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    // Checks
    require!(
        ctx.accounts.authority.key() == ctx.accounts.global_config.authority,
        CoordinationError::NotAuthority,
    );

    let src = &ctx.accounts.src_tournament;
    let dest = &ctx.accounts.dest_tournament;

    require!(
        src.tournament_id != dest.tournament_id,
        CoordinationError::DestTournamentInvalid,
    );
    require!(src.finalized, CoordinationError::TournamentNotFinalized);

    let grace_deadline = src
        .end_time
        .checked_add(UNCLAIMED_GRACE_SECS)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    require!(
        now > grace_deadline,
        CoordinationError::UnclaimedGracePeriodNotElapsed,
    );

    require!(!dest.finalized, CoordinationError::DestTournamentInvalid,);
    require!(
        dest.is_active(now),
        CoordinationError::DestTournamentInvalid,
    );

    let rent = Rent::get()?;
    let rent_minimum = rent.minimum_balance(Tournament::SPACE);
    let src_info = src.to_account_info();
    let src_lamports = src_info.lamports();
    let sweepable = src_lamports
        .checked_sub(rent_minimum)
        .ok_or(CoordinationError::EmptyPrizePool)?;
    require!(sweepable > 0, CoordinationError::EmptyPrizePool);

    let src_id = src.tournament_id;
    let dest_id = dest.tournament_id;

    // Effects: update dest accounting first (CEI ordering)
    let dest_mut = &mut ctx.accounts.dest_tournament;
    dest_mut.prize_lamports = dest_mut
        .prize_lamports
        .checked_add(sweepable)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    // Interactions: move lamports
    let src_info = ctx.accounts.src_tournament.to_account_info();
    let dest_info = ctx.accounts.dest_tournament.to_account_info();

    let src_new = src_info
        .lamports()
        .checked_sub(sweepable)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    let dest_new = dest_info
        .lamports()
        .checked_add(sweepable)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    **src_info.try_borrow_mut_lamports()? = src_new;
    **dest_info.try_borrow_mut_lamports()? = dest_new;

    // Postconditions
    require!(
        ctx.accounts.src_tournament.to_account_info().lamports() == rent_minimum,
        CoordinationError::InvalidGameState,
    );

    emit!(UnclaimedSwept {
        src_tournament_id: src_id,
        dest_tournament_id: dest_id,
        amount: sweepable,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct SweepUnclaimedToNext<'info> {
    #[account(
        mut,
        seeds = [
            b"tournament",
            src_tournament.tournament_id.to_le_bytes().as_ref(),
        ],
        bump = src_tournament.bump,
    )]
    pub src_tournament: Account<'info, Tournament>,

    #[account(
        mut,
        seeds = [
            b"tournament",
            dest_tournament.tournament_id.to_le_bytes().as_ref(),
        ],
        bump = dest_tournament.bump,
    )]
    pub dest_tournament: Account<'info, Tournament>,

    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    pub authority: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grace_period_is_90_days() {
        assert_eq!(UNCLAIMED_GRACE_SECS, 90 * 24 * 60 * 60);
        assert_eq!(UNCLAIMED_GRACE_SECS, 7_776_000);
    }

    #[test]
    fn grace_deadline_arithmetic_holds() {
        let end_time: i64 = 1_777_657_939; // T#1's actual end_time
        let deadline = end_time.checked_add(UNCLAIMED_GRACE_SECS).unwrap();
        assert!(deadline > end_time);
        // 90 days after T#1 ended is roughly 2026-07-30
        assert_eq!(deadline, 1_785_433_939);
    }
}
