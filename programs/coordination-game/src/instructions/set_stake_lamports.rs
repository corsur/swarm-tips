//! `set_stake_lamports` — re-peg the per-game Solana stake without a program
//! upgrade. The Solana counterpart of the EVM contract's `setConfig`.
//!
//! Why it exists: the stake was a compile-time constant, so changing it meant
//! shipping a new program while every EVM chain re-pegged with a transaction.
//! The predictable result was that Solana drifted — 0.05 SOL ($3.64) against a
//! $5 EVM anchor — because one side was cheap to fix and the other was not.
//!
//! Re-pegging is NOT retroactive. Games and escrows already funded at the old
//! stake keep it: `Game.stake_lamports` is recorded per game, and an escrow at
//! a superseded amount simply stops validating for new games (the player
//! withdraws and re-deposits). Nothing in flight is repriced underneath a
//! player.

use anchor_lang::prelude::*;

use crate::errors::CoordinationError;
use crate::events::StakeConfigured;
use crate::state::{GlobalConfig, MAX_STAKE_LAMPORTS, MIN_STAKE_LAMPORTS};

pub fn set_stake_lamports(ctx: Context<SetStakeLamports>, new_stake: u64) -> Result<()> {
    let config = &mut ctx.accounts.global_config;
    let previous = config.stake_lamports;

    // Checks — bounded rather than trusted. An authority signature says the
    // change is authorised, not that the number is sane; a fat-fingered stake
    // is exactly the failure this instruction exists to make cheap to fix, so
    // it must not also be cheap to cause.
    require!(
        (MIN_STAKE_LAMPORTS..=MAX_STAKE_LAMPORTS).contains(&new_stake),
        CoordinationError::StakeMismatch
    );
    require!(new_stake != previous, CoordinationError::StakeMismatch);

    // Effects
    config.stake_lamports = new_stake;

    // Postcondition
    require!(
        config.stake_lamports == new_stake,
        CoordinationError::StakeMismatch
    );

    // Interactions
    emit!(StakeConfigured {
        previous_lamports: previous,
        new_lamports: new_stake,
        authority: ctx.accounts.authority.key(),
    });
    msg!("set_stake_lamports: {} -> {}", previous, new_stake);
    Ok(())
}

#[derive(Accounts)]
pub struct SetStakeLamports<'info> {
    #[account(
        mut,
        seeds = [b"global_config"],
        bump = global_config.bump,
        has_one = authority @ CoordinationError::NotAuthority,
    )]
    pub global_config: Account<'info, GlobalConfig>,
    pub authority: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use crate::state::{MAX_STAKE_LAMPORTS, MIN_STAKE_LAMPORTS};

    /// Mirrors the handler's bound check. Kept pure so the policy is testable
    /// without an Anchor Context.
    fn stake_is_acceptable(new_stake: u64, current: u64) -> bool {
        (MIN_STAKE_LAMPORTS..=MAX_STAKE_LAMPORTS).contains(&new_stake) && new_stake != current
    }

    #[test]
    fn accepts_a_re_peg_inside_the_bounds() {
        // 0.0686 SOL — the $5 anchor at SOL ~$73, i.e. the change this exists for.
        assert!(stake_is_acceptable(68_600_000, 50_000_000));
    }

    #[test]
    fn rejects_a_stake_below_the_floor() {
        // A stake of ~nothing makes the game free to play and breaks the
        // negative-sum economics that bound the FX-freeze exploit.
        assert!(!stake_is_acceptable(MIN_STAKE_LAMPORTS - 1, 50_000_000));
        assert!(stake_is_acceptable(MIN_STAKE_LAMPORTS, 50_000_000));
    }

    #[test]
    fn rejects_a_stake_above_the_ceiling() {
        // Guards the fat-finger case: an authority signature authorises the
        // change, it does not make the number sane.
        assert!(!stake_is_acceptable(MAX_STAKE_LAMPORTS + 1, 50_000_000));
        assert!(stake_is_acceptable(MAX_STAKE_LAMPORTS, 50_000_000));
    }

    #[test]
    fn rejects_a_no_op_re_peg() {
        // A "change" to the current value would emit a StakeConfigured event
        // claiming a peg happened when nothing moved — misleading an indexer
        // reconstructing peg history.
        assert!(!stake_is_acceptable(50_000_000, 50_000_000));
    }
}
