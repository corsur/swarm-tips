// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title PayoutReference
/// @notice The ONE independent Solidity mirror of `scoring.rs::compute_payment`.
///
/// @dev Two sibling tests pin the payout formula and both must derive it the
///      same way, or the transitive claim they make together — fixture ==
///      reference == on-chain `_computePayment` — has a gap in the middle:
///
///        - ShillbotEscrowVectors.t.sol  : fixture  == reference
///        - ShillbotEscrowInvariant.t.sol: reference == on-chain
///
///      They previously carried private copies (`_ref` and `_reference`) that
///      had already drifted: only one guarded `threshold >= MAX_SCORE`, so the
///      other would divide by zero at `threshold == MAX_SCORE` and was saved
///      only by its fuzz bound of 999_999. Keep this the single copy.
library PayoutReference {
    uint256 internal constant MAX_SCORE = 1_000_000;
    uint256 internal constant BPS_DENOM = 10_000;

    /// @notice Reference payout split for a score against a threshold.
    /// @dev Below threshold, or a threshold that leaves no range, pays nothing
    ///      and returns the whole escrow as remainder. uint256 intermediates
    ///      are exact over the contract's ranges (u128 in the Rust original).
    function compute(uint256 score, uint256 threshold, uint256 escrow, uint256 feeBps)
        internal
        pure
        returns (uint256 payment, uint256 fee, uint256 remainder)
    {
        if (score < threshold || threshold >= MAX_SCORE) {
            return (0, 0, escrow);
        }

        uint256 range = MAX_SCORE - threshold;
        uint256 gross = (escrow * (score - threshold)) / range;
        fee = (gross * feeBps) / BPS_DENOM;
        payment = gross - fee;
        remainder = escrow - payment - fee;
    }
}
