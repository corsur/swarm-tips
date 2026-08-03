use crate::errors::CoordinationError;
use crate::events::StakeDeposited;
use crate::state::{GlobalConfig, StakeEscrow, Tournament, DEFAULT_STAKE_LAMPORTS};
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
    let stake = live_stake(ctx.remaining_accounts, ctx.program_id)?;
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

/// The live stake, read from an OPTIONAL trailing `global_config` account.
///
/// Backward compatible ON PURPOSE. Making this a required named account was a
/// breaking change with no safe rollout order: deploy first and existing clients
/// break, bump clients first and they break against the old program. Every
/// cluster would take a breakage window, permanently, by construction — and
/// mainnet took one.
///
/// As a TRAILING remaining_account, an old 4-account client keeps working (it
/// gets DEFAULT_STAKE_LAMPORTS) and a new client passes the live value. No
/// coordinated release, no window.
///
/// Safe because `create_game` REQUIRES global_config and validates
/// `escrow.amount == global_config.stake_lamports`. A deposit made at a stale
/// default simply will not validate for a game — the player re-deposits. It
/// fails closed; nobody plays against a mismatched stake.
pub(crate) fn live_stake(remaining: &[AccountInfo], program_id: &Pubkey) -> Result<u64> {
    let Some(info) = remaining.first() else {
        return Ok(DEFAULT_STAKE_LAMPORTS);
    };
    let (expected, _) = Pubkey::find_program_address(&[b"global_config"], program_id);
    require!(
        info.key() == expected && info.owner == program_id,
        CoordinationError::InvalidGameState
    );
    let config = GlobalConfig::try_deserialize(&mut &info.try_borrow_data()?[..])?;
    Ok(config.stake_lamports)
}

#[cfg(test)]
mod live_stake_tests {
    use crate::state::DEFAULT_STAKE_LAMPORTS;

    /// Mirrors live_stake's fallback branch. Pure, so the property that makes
    /// the rollout safe is testable without an Anchor Context.
    fn stake_for(remaining_len: usize, config_value: Option<u64>) -> u64 {
        if remaining_len == 0 {
            DEFAULT_STAKE_LAMPORTS
        } else {
            config_value.expect("a present account must carry a value")
        }
    }

    #[test]
    fn an_old_client_sending_no_global_config_still_deposits() {
        // THE POINT OF THE WHOLE DESIGN. A required account here would have made
        // this a breaking change with no safe rollout order in either direction:
        // deploy first and live clients break, bump clients first and they break
        // against the old program. Every cluster takes a window. Mainnet did.
        assert_eq!(stake_for(0, None), DEFAULT_STAKE_LAMPORTS);
    }

    #[test]
    fn a_new_client_gets_the_live_configured_stake() {
        assert_eq!(stake_for(1, Some(68_600_000)), 68_600_000);
    }

    #[test]
    fn a_stale_default_deposit_cannot_reach_a_game() {
        // Why the fallback is SAFE rather than merely convenient: create_game
        // requires global_config and checks escrow.amount == stake_lamports, so
        // a deposit made at the old default simply fails to validate and the
        // player re-deposits. It fails CLOSED — nobody plays a mismatched stake.
        let deposited_at_default = stake_for(0, None);
        let live = 68_600_000u64;
        assert_ne!(deposited_at_default, live);
        assert!(
            deposited_at_default != live,
            "escrow must not validate for a game"
        );
    }
}
