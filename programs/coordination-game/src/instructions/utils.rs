use crate::errors::CoordinationError;
use anchor_lang::prelude::*;

/// Transfer lamports directly between two program-owned or system accounts.
///
/// Used by reveal_guess, resolve_timeout, and claim_reward to move lamports
/// out of PDAs. The caller is responsible for ensuring `from` has sufficient
/// balance; this function only performs the checked arithmetic and borrow.
pub fn transfer_lamports(from: &AccountInfo, to: &AccountInfo, lamports: u64) -> Result<()> {
    if lamports == 0 {
        return Ok(());
    }
    **from.try_borrow_mut_lamports()? = from
        .lamports()
        .checked_sub(lamports)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    **to.try_borrow_mut_lamports()? = to
        .lamports()
        .checked_add(lamports)
        .ok_or(CoordinationError::ArithmeticOverflow)?;
    Ok(())
}

/// Split result from `compute_treasury_split`.
pub struct TreasurySplit {
    pub treasury_share: u64,
    pub tournament_share: u64,
}

/// Compute the treasury/tournament split for pool gains.
///
/// Uses u128 intermediate to prevent overflow on large amounts.
/// Postcondition: `treasury_share + tournament_share == total`.
pub fn compute_treasury_split(total: u64, treasury_split_bps: u16) -> Result<TreasurySplit> {
    // Preconditions: total > 0 and bps within valid range
    require!(total > 0, CoordinationError::ArithmeticOverflow);
    require!(
        (crate::state::MIN_TREASURY_SPLIT_BPS..=crate::state::MAX_TREASURY_SPLIT_BPS)
            .contains(&treasury_split_bps),
        CoordinationError::InvalidTreasurySplitBps,
    );

    let total_128 = u128::from(total);
    let bps_128 = u128::from(treasury_split_bps);
    let treasury_128 = total_128
        .checked_mul(bps_128)
        .ok_or(CoordinationError::ArithmeticOverflow)?
        .checked_div(10_000)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    let treasury_share =
        u64::try_from(treasury_128).map_err(|_| CoordinationError::ArithmeticOverflow)?;
    let tournament_share = total
        .checked_sub(treasury_share)
        .ok_or(CoordinationError::ArithmeticOverflow)?;

    // Postcondition: shares sum to total
    require!(
        treasury_share
            .checked_add(tournament_share)
            .ok_or(CoordinationError::ArithmeticOverflow)?
            == total,
        CoordinationError::ArithmeticOverflow,
    );

    Ok(TreasurySplit {
        treasury_share,
        tournament_share,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_50_50_even_amount() {
        // 100_000_000 lamports at 5000 bps = 50/50
        let result = compute_treasury_split(100_000_000, 5_000).unwrap();
        assert_eq!(result.treasury_share, 50_000_000);
        assert_eq!(result.tournament_share, 50_000_000);
        assert_eq!(result.treasury_share + result.tournament_share, 100_000_000);
    }

    #[test]
    fn split_20_80_minimum_bps() {
        // 100_000_000 at 2000 bps (minimum) = 20% treasury, 80% tournament
        let result = compute_treasury_split(100_000_000, 2_000).unwrap();
        assert_eq!(result.treasury_share, 20_000_000);
        assert_eq!(result.tournament_share, 80_000_000);
    }

    #[test]
    fn split_80_20_maximum_bps() {
        // 100_000_000 at 8000 bps (maximum) = 80% treasury, 20% tournament
        let result = compute_treasury_split(100_000_000, 8_000).unwrap();
        assert_eq!(result.treasury_share, 80_000_000);
        assert_eq!(result.tournament_share, 20_000_000);
    }

    #[test]
    fn split_odd_amount_truncates_treasury_remainder_to_tournament() {
        // 3 lamports at 5000 bps: treasury = 3*5000/10000 = 1, tournament = 2
        let result = compute_treasury_split(3, 5_000).unwrap();
        assert_eq!(result.treasury_share, 1);
        assert_eq!(result.tournament_share, 2);
        assert_eq!(result.treasury_share + result.tournament_share, 3);
    }

    #[test]
    fn split_single_lamport() {
        // 1 lamport at 5000 bps: treasury = 0, tournament = 1
        let result = compute_treasury_split(1, 5_000).unwrap();
        assert_eq!(result.treasury_share, 0);
        assert_eq!(result.tournament_share, 1);
    }

    #[test]
    fn split_large_amount_no_overflow() {
        // u64::MAX / 2 to avoid overflow in the test assertion itself
        let large = u64::MAX / 2;
        let result = compute_treasury_split(large, 5_000).unwrap();
        assert_eq!(result.treasury_share + result.tournament_share, large);
    }

    #[test]
    fn split_rejects_zero_total() {
        let result = compute_treasury_split(0, 5_000);
        assert!(result.is_err());
    }

    #[test]
    fn split_rejects_bps_below_minimum() {
        let result = compute_treasury_split(100_000, 1_999);
        assert!(result.is_err());
    }

    #[test]
    fn split_rejects_bps_above_maximum() {
        let result = compute_treasury_split(100_000, 8_001);
        assert!(result.is_err());
    }

    #[test]
    fn split_typical_game_stake() {
        // 0.05 SOL = 50_000_000 lamports per player, 2S = 100_000_000
        // At default 5000 bps: 50M treasury, 50M tournament
        let two_s = 100_000_000u64;
        let result = compute_treasury_split(two_s, 5_000).unwrap();
        assert_eq!(result.treasury_share, 50_000_000);
        assert_eq!(result.tournament_share, 50_000_000);
    }
}
