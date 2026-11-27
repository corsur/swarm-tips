#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

//! Pure `no_std`, overflow-checked settlement and accounting core for Swarm Tips
//! Solana programs (`extension-credit`, `shillbot`, `coordination-game`).
//!
//! This file is the single source of truth mechanically extracted via
//! `cargo xtask extract-protocol-math-lean` into
//! `SwarmProofs.Chains.ProtocolMath.Defs.V1`. Any edit here without re-extracting
//! fails CI (`extract-protocol-math-lean --check`), eliminating the
//! hand-transcription gap between production Rust arithmetic and Lean 4 proofs.

pub const BPS_DENOMINATOR: u64 = 10_000;
pub const MAX_PROTOCOL_FEE_BPS: u64 = 1_000;

/// Result of splitting routed earnings on an `extension-credit` `Advance` vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvanceSplit {
    pub to_backer: u64,
    pub to_recipient: u64,
    pub new_recouped: u64,
    pub remaining_balance: u64,
}

/// Exact `extension-credit::route_and_recoup` arithmetic split:
/// distributes `balance - rent_floor` backer-first up to `outstanding = advance_lamports - recouped_lamports`,
/// sending any remainder to `recipient` and preserving `rent_floor` in the vault.
pub fn compute_route_and_recoup(
    balance: u64,
    rent_floor: u64,
    advance_lamports: u64,
    recouped_lamports: u64,
) -> Option<AdvanceSplit> {
    let available = balance.saturating_sub(rent_floor);
    let outstanding = advance_lamports.checked_sub(recouped_lamports)?;
    let to_backer = available.min(outstanding);
    let to_recipient = available.checked_sub(to_backer)?;
    let new_recouped = recouped_lamports.checked_add(to_backer)?;
    let after_backer = balance.checked_sub(to_backer)?;
    let remaining_balance = after_backer.checked_sub(to_recipient)?;
    Some(AdvanceSplit {
        to_backer,
        to_recipient,
        new_recouped,
        remaining_balance,
    })
}

/// Exact `extension-credit::mark_default` arithmetic split:
/// rejects already-recouped advances (`outstanding == 0`), sweeps routed earnings
/// to `backer` capped at `outstanding`, returns any surplus above `outstanding` to
/// `recipient` (preventing backer front-running), and leaves `rent_floor` for `close = backer`.
pub fn compute_mark_default(
    balance: u64,
    rent_floor: u64,
    advance_lamports: u64,
    recouped_lamports: u64,
) -> Option<AdvanceSplit> {
    let available = balance.saturating_sub(rent_floor);
    let outstanding = advance_lamports.checked_sub(recouped_lamports)?;
    if outstanding == 0 {
        return None;
    }
    let to_backer = available.min(outstanding);
    let to_recipient = available.checked_sub(to_backer)?;
    let new_recouped = recouped_lamports
        .saturating_add(to_backer)
        .min(advance_lamports);
    let after_backer = balance.checked_sub(to_backer)?;
    let remaining_balance = after_backer.checked_sub(to_recipient)?;
    Some(AdvanceSplit {
        to_backer,
        to_recipient,
        new_recouped,
        remaining_balance,
    })
}

/// Result of computing a finalized `shillbot` task payout split from escrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskPayoutSplit {
    pub payment_amount: u64,
    pub fee_amount: u64,
    pub client_refund: u64,
}

/// Exact `shillbot` score-proportional escrow payout split with `u128` intermediate
/// precision: below `quality_threshold`, 100% refunds to client; otherwise
/// `gross = floor(escrow * score / max_score)`, `fee = floor(gross * fee_bps / 10_000)`,
/// `payment = gross - fee`, and `client_refund = escrow - gross`.
pub fn compute_task_payout(
    escrow_lamports: u64,
    score: u64,
    quality_threshold: u64,
    max_score: u64,
    fee_bps: u64,
) -> Option<TaskPayoutSplit> {
    if max_score == 0 {
        return None;
    }
    if score > max_score {
        return None;
    }
    if fee_bps > MAX_PROTOCOL_FEE_BPS {
        return None;
    }
    if score < quality_threshold {
        return Some(TaskPayoutSplit {
            payment_amount: 0,
            fee_amount: 0,
            client_refund: escrow_lamports,
        });
    }
    let gross = mul_div_u64(escrow_lamports, score, max_score)?;
    let fee_amount = mul_div_u64(gross, fee_bps, BPS_DENOMINATOR)?;
    let payment_amount = gross.checked_sub(fee_amount)?;
    let total_out = payment_amount.checked_add(fee_amount)?;
    let client_refund = escrow_lamports.checked_sub(total_out)?;
    Some(TaskPayoutSplit {
        payment_amount,
        fee_amount,
        client_refund,
    })
}

/// Terminal outcome classes for a `coordination-game` cross-chain match leg.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XOutcome {
    BothWin,
    P1Wins,
    P2Wins,
    BothLose,
}

/// Per-leg settlement distribution for `coordination-game::resolve_xleg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XLegSplit {
    pub to_player: u64,
    pub to_fee: u64,
    pub to_prize: u64,
    pub to_pool_reimburse: u64,
    pub tranche_released: u64,
}

/// Exact `coordination-game` cross-chain leg settlement split (`fee_bps <= 1000`).
pub fn compute_xleg_split(
    stake: u64,
    tranche: u64,
    fee_bps: u64,
    seat_is_p1: bool,
    outcome: XOutcome,
) -> Option<XLegSplit> {
    if fee_bps > MAX_PROTOCOL_FEE_BPS {
        return None;
    }
    let fee_cut = mul_div_u64(stake, fee_bps, BPS_DENOMINATOR)?;
    let net_stake = stake.checked_sub(fee_cut)?;
    match outcome {
        XOutcome::BothWin => Some(XLegSplit {
            to_player: stake,
            to_fee: 0,
            to_prize: 0,
            to_pool_reimburse: 0,
            tranche_released: tranche,
        }),
        XOutcome::P1Wins => {
            if seat_is_p1 {
                let total_win = stake.checked_add(tranche)?;
                Some(XLegSplit {
                    to_player: total_win,
                    to_fee: 0,
                    to_prize: 0,
                    to_pool_reimburse: 0,
                    tranche_released: 0,
                })
            } else {
                Some(XLegSplit {
                    to_player: 0,
                    to_fee: fee_cut,
                    to_prize: 0,
                    to_pool_reimburse: net_stake,
                    tranche_released: tranche,
                })
            }
        }
        XOutcome::P2Wins => {
            if seat_is_p1 {
                Some(XLegSplit {
                    to_player: 0,
                    to_fee: fee_cut,
                    to_prize: 0,
                    to_pool_reimburse: net_stake,
                    tranche_released: tranche,
                })
            } else {
                let total_win = stake.checked_add(tranche)?;
                Some(XLegSplit {
                    to_player: total_win,
                    to_fee: 0,
                    to_prize: 0,
                    to_pool_reimburse: 0,
                    tranche_released: 0,
                })
            }
        }
        XOutcome::BothLose => Some(XLegSplit {
            to_player: 0,
            to_fee: fee_cut,
            to_prize: net_stake,
            to_pool_reimburse: 0,
            tranche_released: tranche,
        }),
    }
}

/// Checked `floor(a * b / denom)` computed in `u128`, returning `None` if `denom == 0`
/// or the quotient exceeds `u64::MAX`.
pub fn mul_div_u64(a: u64, b: u64, denom: u64) -> Option<u64> {
    if denom == 0 {
        return None;
    }
    let prod = (a as u128).checked_mul(b as u128)?;
    let quot = prod.checked_div(denom as u128)?;
    u64::try_from(quot).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_and_recoup_conserves_lamports_and_caps_backer_recoupment() {
        let s = compute_route_and_recoup(6_624_280, 1_224_280, 2_600_000, 0).unwrap();
        assert_eq!(s.to_backer, 2_600_000);
        assert_eq!(s.to_recipient, 2_800_000);
        assert_eq!(s.new_recouped, 2_600_000);
        assert_eq!(s.remaining_balance, 1_224_280);
        assert_eq!(
            s.to_backer + s.to_recipient + s.remaining_balance,
            6_624_280
        );

        // Over-recouped corrupted state fails closed with None.
        assert!(compute_route_and_recoup(2_000_000, 1_224_280, 100, 101).is_none());
    }

    #[test]
    fn mark_default_rejects_fully_recouped_and_protects_recipient_surplus() {
        assert!(compute_mark_default(1_224_280, 1_224_280, 2_600_000, 2_600_000).is_none());
        let s = compute_mark_default(6_624_280, 1_224_280, 2_600_000, 1_000_000).unwrap();
        assert_eq!(s.to_backer, 1_600_000);
        assert_eq!(s.to_recipient, 3_800_000);
        assert_eq!(s.new_recouped, 2_600_000);
        assert_eq!(s.remaining_balance, 1_224_280);
    }

    #[test]
    fn task_payout_handles_full_threshold_zero_and_u64_max_escrow() {
        let full = compute_task_payout(10_000_000, 1_000_000, 700_000, 1_000_000, 1_000).unwrap();
        assert_eq!(full.payment_amount, 9_000_000);
        assert_eq!(full.fee_amount, 1_000_000);
        assert_eq!(full.client_refund, 0);

        let below = compute_task_payout(10_000_000, 699_999, 700_000, 1_000_000, 1_000).unwrap();
        assert_eq!(below.payment_amount, 0);
        assert_eq!(below.fee_amount, 0);
        assert_eq!(below.client_refund, 10_000_000);

        // u64::MAX escrow must not overflow intermediate u128 multiplication.
        let max = compute_task_payout(u64::MAX, 1_000_000, 1, 1_000_000, 1_000).unwrap();
        assert_eq!(
            u128::from(max.payment_amount)
                + u128::from(max.fee_amount)
                + u128::from(max.client_refund),
            u128::from(u64::MAX)
        );

        // Invalid parameters fail closed.
        assert!(compute_task_payout(1_000, 1, 0, 0, 100).is_none());
        assert!(compute_task_payout(1_000, 11, 0, 10, 100).is_none());
        assert!(compute_task_payout(1_000, 5, 0, 10, 1_001).is_none());
    }

    #[test]
    fn xleg_split_rejects_winner_overflow_and_conserves_value() {
        let win = compute_xleg_split(1_000_000, 900_000, 1_000, true, XOutcome::P1Wins).unwrap();
        assert_eq!(win.to_player, 1_900_000);
        assert_eq!(win.tranche_released, 0);

        let lose = compute_xleg_split(1_000_000, 900_000, 1_000, false, XOutcome::P1Wins).unwrap();
        assert_eq!(lose.to_fee, 100_000);
        assert_eq!(lose.to_pool_reimburse, 900_000);
        assert_eq!(lose.tranche_released, 900_000);

        // Overflow on `stake + tranche` returns None rather than wrapping.
        assert!(compute_xleg_split(u64::MAX, 1, 1_000, true, XOutcome::P1Wins).is_none());
    }
}
