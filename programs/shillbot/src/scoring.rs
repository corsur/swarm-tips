use crate::errors::ShillbotError;
use anchor_lang::prelude::*;
use shared::MAX_SCORE;

/// Computes the payment amount given the composite score, quality threshold,
/// escrow amount, and protocol fee in basis points.
///
/// Formula:
///   if composite_score < quality_threshold: payment = 0, fee = 0
///   else:
///     score_range = MAX_SCORE - quality_threshold
///     score_above = composite_score - quality_threshold
///     gross_payment = escrow * score_above / score_range
///     fee = gross_payment * protocol_fee_bps / 10_000
///     payment = gross_payment - fee
///
/// All arithmetic uses checked operations with u128 intermediates.
/// Returns (payment_amount, fee_amount).
pub fn compute_payment(
    composite_score: u64,
    quality_threshold: u64,
    escrow_lamports: u64,
    protocol_fee_bps: u16,
) -> Result<(u64, u64)> {
    // Precondition: score within bounds
    require!(
        composite_score <= MAX_SCORE,
        ShillbotError::ScoreOutOfBounds
    );
    // Precondition: threshold within bounds
    require!(
        quality_threshold <= MAX_SCORE,
        ShillbotError::ScoreOutOfBounds
    );

    if composite_score < quality_threshold {
        return Ok((0, 0));
    }

    let score_range = MAX_SCORE
        .checked_sub(quality_threshold)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Edge case: threshold == MAX_SCORE means no payment is possible for any score.
    // score_range == 0 would cause division by zero.
    if score_range == 0 {
        return Ok((0, 0));
    }

    let score_above = composite_score
        .checked_sub(quality_threshold)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // gross_payment = escrow * score_above / score_range (u128 intermediate)
    let numerator = (escrow_lamports as u128)
        .checked_mul(score_above as u128)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let gross_payment_128 = numerator
        .checked_div(score_range as u128)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let gross_payment =
        u64::try_from(gross_payment_128).map_err(|_| error!(ShillbotError::ArithmeticOverflow))?;

    // fee = gross_payment * protocol_fee_bps / 10_000 (u128 intermediate)
    let fee_numerator = (gross_payment as u128)
        .checked_mul(protocol_fee_bps as u128)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let fee_128 = fee_numerator
        .checked_div(10_000u128)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    let fee = u64::try_from(fee_128).map_err(|_| error!(ShillbotError::ArithmeticOverflow))?;

    let payment = gross_payment
        .checked_sub(fee)
        .ok_or(ShillbotError::ArithmeticOverflow)?;

    // Postcondition: payment + fee must not exceed escrow
    let total_out = payment
        .checked_add(fee)
        .ok_or(ShillbotError::ArithmeticOverflow)?;
    require!(
        total_out <= escrow_lamports,
        ShillbotError::PaymentExceedsEscrow
    );

    Ok((payment, fee))
}

/// Computes the minimum challenge bond: multiplier * escrow_lamports.
/// Multiplier must be in [MIN_CHALLENGE_BOND_MULTIPLIER, MAX_CHALLENGE_BOND_MULTIPLIER].
pub fn compute_challenge_bond(escrow_lamports: u64, multiplier: u8) -> Result<u64> {
    // Precondition: multiplier within spec bounds
    require!(
        multiplier >= crate::MIN_CHALLENGE_BOND_MULTIPLIER,
        ShillbotError::InsufficientBond
    );
    require!(
        multiplier <= crate::MAX_CHALLENGE_BOND_MULTIPLIER,
        ShillbotError::InsufficientBond
    );

    escrow_lamports
        .checked_mul(multiplier as u64)
        .ok_or_else(|| error!(ShillbotError::ArithmeticOverflow))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Payment computation tests ---

    #[test]
    fn payment_below_threshold_returns_zero() {
        let (payment, fee) = compute_payment(100_000, 200_000, 1_000_000, 1000).unwrap();
        assert_eq!(payment, 0);
        assert_eq!(fee, 0);
    }

    #[test]
    fn payment_at_threshold_returns_zero() {
        let (payment, fee) = compute_payment(200_000, 200_000, 1_000_000, 1000).unwrap();
        assert_eq!(payment, 0);
        assert_eq!(fee, 0);
    }

    #[test]
    fn payment_at_max_score_returns_full_minus_fee() {
        // score = MAX_SCORE, threshold = 200_000, escrow = 1_000_000, fee = 10%
        let (payment, fee) = compute_payment(1_000_000, 200_000, 1_000_000, 1000).unwrap();
        // gross = 1_000_000 * 800_000 / 800_000 = 1_000_000
        // fee = 1_000_000 * 1000 / 10_000 = 100_000
        // payment = 900_000
        assert_eq!(fee, 100_000);
        assert_eq!(payment, 900_000);
    }

    #[test]
    fn payment_midpoint_score() {
        // score = 600_000, threshold = 200_000, escrow = 1_000_000, fee = 10%
        let (payment, fee) = compute_payment(600_000, 200_000, 1_000_000, 1000).unwrap();
        // score_above = 400_000, score_range = 800_000
        // gross = 1_000_000 * 400_000 / 800_000 = 500_000
        // fee = 500_000 * 1000 / 10_000 = 50_000
        // payment = 450_000
        assert_eq!(fee, 50_000);
        assert_eq!(payment, 450_000);
    }

    #[test]
    fn payment_zero_fee_bps() {
        let (payment, fee) = compute_payment(1_000_000, 200_000, 1_000_000, 0).unwrap();
        assert_eq!(fee, 0);
        assert_eq!(payment, 1_000_000);
    }

    #[test]
    fn payment_zero_escrow() {
        let (payment, fee) = compute_payment(500_000, 200_000, 0, 1000).unwrap();
        assert_eq!(payment, 0);
        assert_eq!(fee, 0);
    }

    #[test]
    fn payment_zero_score() {
        let (payment, fee) = compute_payment(0, 200_000, 1_000_000, 1000).unwrap();
        assert_eq!(payment, 0);
        assert_eq!(fee, 0);
    }

    #[test]
    fn payment_threshold_equals_max_score() {
        // score_range = 0, should return (0, 0) to avoid division by zero
        let (payment, fee) = compute_payment(1_000_000, 1_000_000, 1_000_000, 1000).unwrap();
        assert_eq!(payment, 0);
        assert_eq!(fee, 0);
    }

    #[test]
    fn payment_plus_fee_never_exceeds_escrow() {
        // Test with large values that could overflow without u128
        let escrow = u64::MAX / 2;
        let (payment, fee) = compute_payment(1_000_000, 0, escrow, 2500).unwrap();
        let total = payment.checked_add(fee).unwrap();
        assert!(total <= escrow);
    }

    #[test]
    fn payment_score_exceeds_max_returns_error() {
        let result = compute_payment(1_000_001, 200_000, 1_000_000, 1000);
        assert!(result.is_err());
    }

    #[test]
    fn payment_large_escrow_no_overflow() {
        // u64::MAX as escrow; u128 intermediates prevent overflow
        let escrow = u64::MAX;
        let (payment, fee) = compute_payment(500_000, 0, escrow, 1000).unwrap();
        let total = payment.checked_add(fee).unwrap();
        assert!(total <= escrow);
    }

    #[test]
    fn payment_just_above_threshold() {
        // score = threshold + 1
        let (payment, fee) = compute_payment(200_001, 200_000, 1_000_000_000, 1000).unwrap();
        // score_above = 1, score_range = 800_000
        // gross = 1_000_000_000 * 1 / 800_000 = 1250
        // fee = 1250 * 1000 / 10_000 = 125
        // payment = 1125
        assert_eq!(fee, 125);
        assert_eq!(payment, 1125);
    }

    // --- Challenge bond tests ---

    #[test]
    fn challenge_bond_2x() {
        let bond = compute_challenge_bond(1_000_000, 2).unwrap();
        assert_eq!(bond, 2_000_000);
    }

    #[test]
    fn challenge_bond_5x() {
        let bond = compute_challenge_bond(1_000_000, 5).unwrap();
        assert_eq!(bond, 5_000_000);
    }

    #[test]
    fn challenge_bond_10x() {
        let bond = compute_challenge_bond(1_000_000, 10).unwrap();
        assert_eq!(bond, 10_000_000);
    }

    #[test]
    fn challenge_bond_below_min_multiplier_fails() {
        let result = compute_challenge_bond(1_000_000, 1);
        assert!(result.is_err());
    }

    #[test]
    fn challenge_bond_above_max_multiplier_fails() {
        let result = compute_challenge_bond(1_000_000, 11);
        assert!(result.is_err());
    }

    #[test]
    fn challenge_bond_overflow_fails() {
        let result = compute_challenge_bond(u64::MAX, 5);
        assert!(result.is_err());
    }

    #[test]
    fn challenge_bond_zero_escrow() {
        let bond = compute_challenge_bond(0, 2).unwrap();
        assert_eq!(bond, 0);
    }

    // --- Additional payment edge case tests ---

    #[test]
    fn payment_max_fee_bps() {
        // Max protocol fee = 2500 bps (25%)
        let (payment, fee) = compute_payment(1_000_000, 200_000, 1_000_000, 2500).unwrap();
        // gross = 1_000_000
        // fee = 1_000_000 * 2500 / 10_000 = 250_000
        // payment = 750_000
        assert_eq!(fee, 250_000);
        assert_eq!(payment, 750_000);
        assert!(payment.checked_add(fee).unwrap() <= 1_000_000);
    }

    #[test]
    fn payment_min_fee_bps() {
        // Min protocol fee = 100 bps (1%)
        let (payment, fee) = compute_payment(1_000_000, 200_000, 1_000_000, 100).unwrap();
        // gross = 1_000_000
        // fee = 1_000_000 * 100 / 10_000 = 10_000
        // payment = 990_000
        assert_eq!(fee, 10_000);
        assert_eq!(payment, 990_000);
    }

    #[test]
    fn payment_threshold_zero_full_range() {
        // threshold = 0 means any positive score gets paid
        let (payment, fee) = compute_payment(1, 0, 1_000_000_000, 1000).unwrap();
        // score_above = 1, score_range = 1_000_000
        // gross = 1_000_000_000 * 1 / 1_000_000 = 1000
        // fee = 1000 * 1000 / 10_000 = 100
        // payment = 900
        assert_eq!(fee, 100);
        assert_eq!(payment, 900);
    }

    #[test]
    fn payment_threshold_exceeds_max_score_returns_error() {
        let result = compute_payment(500_000, 1_000_001, 1_000_000, 1000);
        assert!(result.is_err());
    }

    #[test]
    fn challenge_bond_at_min_multiplier() {
        let bond = compute_challenge_bond(500_000, 2).unwrap();
        assert_eq!(bond, 1_000_000);
    }

    #[test]
    fn challenge_bond_at_max_multiplier() {
        let bond = compute_challenge_bond(500_000, 5).unwrap();
        assert_eq!(bond, 2_500_000);
    }

    #[test]
    fn challenge_bond_between_min_and_max_multiplier() {
        let bond = compute_challenge_bond(500_000, 3).unwrap();
        assert_eq!(bond, 1_500_000);
    }
}
