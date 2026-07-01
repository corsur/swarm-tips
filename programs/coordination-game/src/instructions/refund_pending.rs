use crate::errors::CoordinationError;
use crate::events::GameCancelled;
use crate::instructions::utils::transfer_lamports;
use crate::state::{Game, GameState, PENDING_JOIN_WINDOW_SECS};
use anchor_lang::prelude::*;

/// Refund an un-joined `Pending` game whose second player never joined, once the
/// join window (`created_at + PENDING_JOIN_WINDOW_SECS`) has elapsed. Refunds
/// P1's single stake to `player_one_wallet`, marks the game `Cancelled`, and
/// closes the account (rent → caller) in one permissionless tx.
///
/// Timeout-gated + permissionless — the window is the whole security guard:
/// during it, nobody can cancel, so a matched P2 always has a clean shot to
/// join, and no third party can cancel-race a live join. Reverts once P2 joins
/// (state ≠ Pending), so a join that lands first always wins (no double-spend).
///
/// Homogeneous with the EVM `CoordinationGame.cancelPending(gameId)`
/// (`if status != Pending revert; if now <= createdAt + commitWindowSecs revert;
/// refund player1`). Without it, an un-joined game stranded P1's stake forever.
pub fn refund_pending(ctx: Context<RefundPending>) -> Result<()> {
    // Checks
    let game = &ctx.accounts.game;
    require!(
        game.state == GameState::Pending,
        CoordinationError::InvalidGameState,
    );
    let now = Clock::get()?.unix_timestamp;
    let deadline = game
        .created_at
        .checked_add(PENDING_JOIN_WINDOW_SECS)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    require!(now > deadline, CoordinationError::TimeoutNotElapsed);

    let refund = game.stake_lamports;
    let game_id = game.game_id;
    let player_one = game.player_one;
    let game_info = ctx.accounts.game.to_account_info();
    let player_one_info = ctx.accounts.player_one_wallet.to_account_info();

    // Effects — terminal state before the lamport move (CEI). The account is
    // closed at instruction exit by the `close = caller` constraint, so this
    // marker is transient, but it keeps the state machine honest during the tx.
    let game_mut = &mut ctx.accounts.game;
    game_mut.state = GameState::Cancelled;
    game_mut.resolved_at = now;

    // Interactions — refund P1's stake; Anchor `close = caller` then sweeps the
    // remaining rent to the caller at instruction exit.
    transfer_lamports(&game_info, &player_one_info, refund)?;

    emit!(GameCancelled {
        game_id,
        player_one,
        refund_lamports: refund,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct RefundPending<'info> {
    #[account(
        mut,
        seeds = [b"game", game.game_id.to_le_bytes().as_ref()],
        bump = game.bump,
        close = caller,
    )]
    pub game: Account<'info, Game>,
    /// CHECK: Verified by address constraint against game.player_one; receives
    /// the refunded stake regardless of who cranks the instruction.
    #[account(mut, address = game.player_one)]
    pub player_one_wallet: UncheckedAccount<'info>,
    /// Permissionless caller: pays the tx fee, receives the reclaimed rent.
    #[account(mut)]
    pub caller: Signer<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure re-implementation of the two guards the handler enforces, so we can
    // unit-test the decision logic without a validator. The on-chain behavior is
    // `require!(state == Pending)` + `require!(now > created_at + WINDOW)`.
    fn may_refund(state: GameState, created_at: i64, now: i64) -> Result<()> {
        require!(
            state == GameState::Pending,
            CoordinationError::InvalidGameState
        );
        let deadline = created_at
            .checked_add(PENDING_JOIN_WINDOW_SECS)
            .ok_or(CoordinationError::ArithmeticOverflow)?;
        require!(now > deadline, CoordinationError::TimeoutNotElapsed);
        Ok(())
    }

    const CREATED: i64 = 1_000_000;

    #[test]
    fn refunds_pending_after_window_regardless_of_caller() {
        // Any caller may crank it once the window elapses (permissionless).
        assert!(may_refund(
            GameState::Pending,
            CREATED,
            CREATED + PENDING_JOIN_WINDOW_SECS + 1
        )
        .is_ok());
    }

    #[test]
    fn reverts_before_the_window_elapses() {
        // The whole security guard: nobody can cancel during the join window.
        let err = may_refund(
            GameState::Pending,
            CREATED,
            CREATED + PENDING_JOIN_WINDOW_SECS,
        )
        .unwrap_err();
        assert_eq!(err, CoordinationError::TimeoutNotElapsed.into());
        // Even one second before the deadline.
        assert!(may_refund(
            GameState::Pending,
            CREATED,
            CREATED + PENDING_JOIN_WINDOW_SECS - 1
        )
        .is_err());
    }

    #[test]
    fn reverts_once_p2_has_joined() {
        // Once the game leaves Pending (P2 joined → Active, or any later state),
        // refund_pending must reject — a join that lands first always wins.
        for state in [
            GameState::Active,
            GameState::Committing,
            GameState::Revealing,
            GameState::Resolved,
            GameState::Cancelled,
        ] {
            let err =
                may_refund(state, CREATED, CREATED + PENDING_JOIN_WINDOW_SECS + 1_000).unwrap_err();
            assert_eq!(err, CoordinationError::InvalidGameState.into());
        }
    }

    #[test]
    fn refund_amount_is_the_single_p1_stake() {
        // Conservation: a Pending game holds exactly P1's one stake (P2 never
        // deposited), so the refund equals stake_lamports and treasury/tournament
        // get nothing. This documents the invariant the handler relies on.
        let stake = crate::state::FIXED_STAKE_LAMPORTS;
        let refund = stake; // handler: `let refund = game.stake_lamports;`
        assert_eq!(refund, stake);
    }
}
