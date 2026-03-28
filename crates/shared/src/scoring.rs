use borsh::{BorshDeserialize, BorshSerialize};

use crate::constants::{
    BPS_DENOMINATOR, MAX_PENALTY_WEIGHT_BPS, MAX_SCORE, MAX_WEIGHT_BPS, METRIC_COUNT,
    MIN_WEIGHT_BPS,
};

/// Composite score with per-metric breakdown and penalty.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompositeScore {
    /// Total composite score (fixed-point, max = MAX_SCORE).
    pub total: u64,
    /// Per-metric weighted scores (6 metrics).
    pub metric_scores: [u64; METRIC_COUNT],
    /// Bot engagement penalty applied (subtracted from raw total).
    pub penalty: u64,
}

impl CompositeScore {
    /// Validates that the composite score is well-formed.
    ///
    /// Checks:
    /// - total does not exceed MAX_SCORE
    /// - no individual metric score exceeds MAX_SCORE
    /// - penalty does not exceed MAX_SCORE
    /// - total is consistent: sum of metric_scores minus penalty equals total
    ///   (clamped at zero if penalty exceeds raw sum)
    pub fn validate(&self) -> Result<(), &'static str> {
        // Precondition: total within bounds
        if self.total > MAX_SCORE {
            return Err("total exceeds MAX_SCORE");
        }
        // Precondition: each metric score within bounds
        let mut raw_sum: u64 = 0;
        for score in &self.metric_scores {
            if *score > MAX_SCORE {
                return Err("metric score exceeds MAX_SCORE");
            }
            raw_sum = raw_sum.checked_add(*score).ok_or("metric sum overflow")?;
        }
        // Precondition: penalty within bounds
        if self.penalty > MAX_SCORE {
            return Err("penalty exceeds MAX_SCORE");
        }
        // Postcondition: total equals clamped(raw_sum - penalty)
        let expected = raw_sum.saturating_sub(self.penalty);
        // Clamp expected to MAX_SCORE since total must not exceed it
        let expected_clamped = if expected > MAX_SCORE {
            MAX_SCORE
        } else {
            expected
        };
        if self.total != expected_clamped {
            return Err("total does not match metric_scores minus penalty");
        }
        Ok(())
    }
}

/// Scoring weight configuration for the 6 engagement metrics.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ScoringWeights {
    /// Basis points per metric (6 weights). Must sum to 10_000.
    /// Each weight must be in [MIN_WEIGHT_BPS, MAX_WEIGHT_BPS].
    pub weights: [u16; METRIC_COUNT],
    /// Bot penalty weight in basis points.
    /// Must be in [0, MAX_PENALTY_WEIGHT_BPS].
    pub penalty_weight: u16,
}

impl ScoringWeights {
    /// Validates that the scoring weights are well-formed.
    ///
    /// Checks:
    /// - Each weight is within [MIN_WEIGHT_BPS, MAX_WEIGHT_BPS]
    /// - All 6 weights sum to exactly BPS_DENOMINATOR (10_000)
    /// - penalty_weight does not exceed MAX_PENALTY_WEIGHT_BPS
    pub fn validate(&self) -> Result<(), &'static str> {
        // Precondition: each weight within bounds
        let mut sum: u16 = 0;
        for weight in &self.weights {
            if *weight < MIN_WEIGHT_BPS {
                return Err("weight below minimum");
            }
            if *weight > MAX_WEIGHT_BPS {
                return Err("weight above maximum");
            }
            sum = sum.checked_add(*weight).ok_or("weight sum overflow")?;
        }
        // Postcondition: weights sum to denominator
        if sum != BPS_DENOMINATOR {
            return Err("weights do not sum to 10000");
        }
        // Precondition: penalty weight within bounds
        if self.penalty_weight > MAX_PENALTY_WEIGHT_BPS {
            return Err("penalty_weight exceeds maximum");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- CompositeScore tests ---

    fn valid_composite_score() -> CompositeScore {
        // 6 metrics each at 100_000, raw sum = 600_000, penalty = 100_000, total = 500_000
        CompositeScore {
            total: 500_000,
            metric_scores: [100_000; METRIC_COUNT],
            penalty: 100_000,
        }
    }

    #[test]
    fn composite_score_valid() {
        assert!(valid_composite_score().validate().is_ok());
    }

    #[test]
    fn composite_score_zero_valid() {
        let score = CompositeScore {
            total: 0,
            metric_scores: [0; METRIC_COUNT],
            penalty: 0,
        };
        assert!(score.validate().is_ok());
    }

    #[test]
    fn composite_score_total_exceeds_max_rejected() {
        let score = CompositeScore {
            total: MAX_SCORE.checked_add(1).unwrap(),
            metric_scores: [200_000; METRIC_COUNT],
            penalty: 0,
        };
        assert_eq!(score.validate(), Err("total exceeds MAX_SCORE"));
    }

    #[test]
    fn composite_score_metric_exceeds_max_rejected() {
        let mut score = valid_composite_score();
        score.metric_scores[0] = MAX_SCORE.checked_add(1).unwrap();
        assert_eq!(score.validate(), Err("metric score exceeds MAX_SCORE"));
    }

    #[test]
    fn composite_score_penalty_exceeds_max_rejected() {
        let score = CompositeScore {
            total: 0,
            metric_scores: [0; METRIC_COUNT],
            penalty: MAX_SCORE.checked_add(1).unwrap(),
        };
        assert_eq!(score.validate(), Err("penalty exceeds MAX_SCORE"));
    }

    #[test]
    fn composite_score_inconsistent_total_rejected() {
        let mut score = valid_composite_score();
        // Correct total would be 500_000, set it wrong
        score.total = 400_000;
        assert_eq!(
            score.validate(),
            Err("total does not match metric_scores minus penalty")
        );
    }

    #[test]
    fn composite_score_penalty_exceeds_raw_sum_clamps_to_zero() {
        let score = CompositeScore {
            total: 0,
            metric_scores: [10_000; METRIC_COUNT],
            penalty: MAX_SCORE,
        };
        // raw_sum = 60_000, penalty = 1_000_000, clamped to 0
        assert!(score.validate().is_ok());
    }

    #[test]
    fn composite_score_max_valid() {
        let score = CompositeScore {
            total: MAX_SCORE,
            metric_scores: [200_000; METRIC_COUNT],
            penalty: 200_000,
        };
        // raw_sum = 1_200_000, penalty = 200_000, expected = 1_000_000 = MAX_SCORE
        assert!(score.validate().is_ok());
    }

    #[test]
    fn composite_score_raw_sum_exceeds_max_clamped() {
        // raw_sum = 1_200_000, penalty = 0, expected clamped = MAX_SCORE
        let score = CompositeScore {
            total: MAX_SCORE,
            metric_scores: [200_000; METRIC_COUNT],
            penalty: 0,
        };
        assert!(score.validate().is_ok());
    }

    #[test]
    fn composite_score_roundtrip_serialization() {
        let score = valid_composite_score();
        let bytes = borsh::to_vec(&score).expect("serialize");
        let deserialized = CompositeScore::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(score, deserialized);
    }

    // --- ScoringWeights tests ---

    fn valid_weights() -> ScoringWeights {
        // 6 equal weights: 1666 * 4 + 1667 * 2 = 6664 + 3334 = 9998...
        // Let's use a valid distribution: 1667, 1667, 1667, 1667, 1666, 1666 = 10000
        ScoringWeights {
            weights: [1667, 1667, 1667, 1667, 1666, 1666],
            penalty_weight: 1000,
        }
    }

    #[test]
    fn scoring_weights_valid() {
        assert!(valid_weights().validate().is_ok());
    }

    #[test]
    fn scoring_weights_below_minimum_rejected() {
        let mut weights = valid_weights();
        weights.weights[0] = MIN_WEIGHT_BPS.checked_sub(1).unwrap();
        assert_eq!(weights.validate(), Err("weight below minimum"));
    }

    #[test]
    fn scoring_weights_above_maximum_rejected() {
        let mut weights = valid_weights();
        weights.weights[0] = MAX_WEIGHT_BPS.checked_add(1).unwrap();
        assert_eq!(weights.validate(), Err("weight above maximum"));
    }

    #[test]
    fn scoring_weights_sum_not_10000_rejected() {
        let weights = ScoringWeights {
            weights: [1667, 1667, 1667, 1667, 1666, 1665],
            penalty_weight: 1000,
        };
        assert_eq!(weights.validate(), Err("weights do not sum to 10000"));
    }

    #[test]
    fn scoring_weights_sum_over_10000_rejected() {
        let weights = ScoringWeights {
            weights: [1667, 1667, 1667, 1667, 1666, 1667],
            penalty_weight: 1000,
        };
        assert_eq!(weights.validate(), Err("weights do not sum to 10000"));
    }

    #[test]
    fn scoring_weights_penalty_exceeds_max_rejected() {
        let mut weights = valid_weights();
        weights.penalty_weight = MAX_PENALTY_WEIGHT_BPS.checked_add(1).unwrap();
        assert_eq!(weights.validate(), Err("penalty_weight exceeds maximum"));
    }

    #[test]
    fn scoring_weights_penalty_at_max_valid() {
        let mut weights = valid_weights();
        weights.penalty_weight = MAX_PENALTY_WEIGHT_BPS;
        assert!(weights.validate().is_ok());
    }

    #[test]
    fn scoring_weights_penalty_zero_valid() {
        let mut weights = valid_weights();
        weights.penalty_weight = 0;
        assert!(weights.validate().is_ok());
    }

    #[test]
    fn scoring_weights_boundary_all_min() {
        // 6 weights at MIN (500) = 3000, not 10000 — should fail
        let weights = ScoringWeights {
            weights: [MIN_WEIGHT_BPS; METRIC_COUNT],
            penalty_weight: 0,
        };
        assert_eq!(weights.validate(), Err("weights do not sum to 10000"));
    }

    #[test]
    fn scoring_weights_boundary_all_max() {
        // 6 weights at MAX (5000) = 30000, not 10000 — should fail
        let weights = ScoringWeights {
            weights: [MAX_WEIGHT_BPS; METRIC_COUNT],
            penalty_weight: 0,
        };
        // First check: each weight is within bounds (passes)
        // Sum check: 30000 != 10000 — but wait, sum overflows u16!
        // 5000 * 6 = 30000 > u16::MAX (65535) — no overflow, 30000 fits in u16
        assert_eq!(weights.validate(), Err("weights do not sum to 10000"));
    }

    #[test]
    fn scoring_weights_extreme_distribution_valid() {
        // One weight at max (5000), rest distributed to sum to 10000
        // 5000 + 5*1000 = 10000
        let weights = ScoringWeights {
            weights: [5000, 1000, 1000, 1000, 1000, 1000],
            penalty_weight: 500,
        };
        assert!(weights.validate().is_ok());
    }

    #[test]
    fn scoring_weights_roundtrip_serialization() {
        let weights = valid_weights();
        let bytes = borsh::to_vec(&weights).expect("serialize");
        let deserialized = ScoringWeights::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(weights, deserialized);
    }
}
