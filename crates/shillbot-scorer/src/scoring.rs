use crate::errors::ScorerError;
use crate::models::{
    AntiGamingFlag, NormalizedMetrics, ScoringWeights, MAX_SCORE, MAX_WEIGHT, MIN_WEIGHT,
};

/// Compute the composite score from normalized metrics, weights, and anti-gaming flags.
///
/// Formula:
/// ```text
/// score = w_views * norm_views
///       + w_likes * norm_likes
///       + w_comments * norm_comments
///       + w_engagement * norm_engagement_rate
///       + w_watch_proxy * norm_watch_proxy
///       - w_penalty * bot_penalty_factor
/// ```
///
/// All values are fixed-point with MAX_SCORE = 1,000,000.
/// Weights are in [MIN_WEIGHT, MAX_WEIGHT] and represent fractions of MAX_SCORE.
/// The result is clamped to [0, MAX_SCORE].
pub fn compute_composite_score(
    normalized: &NormalizedMetrics,
    weights: &ScoringWeights,
    flags: &[AntiGamingFlag],
) -> Result<u64, ScorerError> {
    validate_weights(weights)?;

    assert!(
        normalized.views <= MAX_SCORE,
        "normalized views exceeds MAX_SCORE"
    );
    assert!(
        normalized.likes <= MAX_SCORE,
        "normalized likes exceeds MAX_SCORE"
    );

    let weighted_views = weighted_component(normalized.views, weights.views)?;
    let weighted_likes = weighted_component(normalized.likes, weights.likes)?;
    let weighted_comments = weighted_component(normalized.comments, weights.comments)?;
    let weighted_engagement = weighted_component(normalized.engagement_rate, weights.engagement)?;
    let weighted_watch = weighted_component(normalized.watch_proxy, weights.watch_proxy)?;

    let gross = weighted_views
        .checked_add(weighted_likes)
        .ok_or(ScorerError::ArithmeticOverflow)?
        .checked_add(weighted_comments)
        .ok_or(ScorerError::ArithmeticOverflow)?
        .checked_add(weighted_engagement)
        .ok_or(ScorerError::ArithmeticOverflow)?
        .checked_add(weighted_watch)
        .ok_or(ScorerError::ArithmeticOverflow)?;

    let penalty = compute_bot_penalty(weights.bot_penalty, flags)?;

    let score = gross.saturating_sub(penalty).min(MAX_SCORE);

    assert!(score <= MAX_SCORE, "composite score exceeds MAX_SCORE");
    Ok(score)
}

/// Compute a single weighted component: (normalized_value * weight) / MAX_SCORE.
fn weighted_component(normalized_value: u64, weight: u64) -> Result<u64, ScorerError> {
    let product = normalized_value
        .checked_mul(weight)
        .ok_or(ScorerError::ArithmeticOverflow)?;
    product
        .checked_div(MAX_SCORE)
        .ok_or(ScorerError::ArithmeticOverflow)
}

/// Compute bot penalty based on flags and penalty weight.
///
/// Each flag contributes proportionally: penalty_per_flag = penalty_weight / 2.
/// Two flags = full penalty weight applied.
fn compute_bot_penalty(penalty_weight: u64, flags: &[AntiGamingFlag]) -> Result<u64, ScorerError> {
    if flags.is_empty() {
        return Ok(0);
    }

    let flag_count = flags.len().min(2) as u64;
    let penalty_per_flag = penalty_weight
        .checked_div(2)
        .ok_or(ScorerError::ArithmeticOverflow)?;
    flag_count
        .checked_mul(penalty_per_flag)
        .ok_or(ScorerError::ArithmeticOverflow)
}

/// Validate that all weights are within allowed bounds.
fn validate_weights(weights: &ScoringWeights) -> Result<(), ScorerError> {
    let all_weights = [
        ("views", weights.views),
        ("likes", weights.likes),
        ("comments", weights.comments),
        ("engagement", weights.engagement),
        ("watch_proxy", weights.watch_proxy),
    ];

    for (name, w) in &all_weights {
        if *w < MIN_WEIGHT || *w > MAX_WEIGHT {
            return Err(ScorerError::InvalidWeights(format!(
                "{name} weight {w} outside range [{MIN_WEIGHT}, {MAX_WEIGHT}]"
            )));
        }
    }

    // bot_penalty weight has its own range (can be 0 to MAX_WEIGHT)
    if weights.bot_penalty > MAX_WEIGHT {
        return Err(ScorerError::InvalidWeights(format!(
            "bot_penalty weight {} exceeds {MAX_WEIGHT}",
            weights.bot_penalty
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NormalizedMetrics;

    fn default_weights() -> ScoringWeights {
        ScoringWeights::default()
    }

    fn max_normalized() -> NormalizedMetrics {
        NormalizedMetrics {
            views: MAX_SCORE,
            likes: MAX_SCORE,
            comments: MAX_SCORE,
            engagement_rate: MAX_SCORE,
            watch_proxy: MAX_SCORE,
        }
    }

    fn zero_normalized() -> NormalizedMetrics {
        NormalizedMetrics {
            views: 0,
            likes: 0,
            comments: 0,
            engagement_rate: 0,
            watch_proxy: 0,
        }
    }

    #[test]
    fn score_zero_metrics() {
        let score = compute_composite_score(&zero_normalized(), &default_weights(), &[]).unwrap();
        assert_eq!(score, 0);
    }

    #[test]
    fn score_max_metrics_no_penalty() {
        let score = compute_composite_score(&max_normalized(), &default_weights(), &[]).unwrap();
        // Sum of positive weights: 200_000 + 150_000 + 150_000 + 250_000 + 200_000 = 950_000
        assert_eq!(score, 950_000);
    }

    #[test]
    fn score_max_metrics_with_one_flag() {
        let flags = vec![AntiGamingFlag::ViewVelocityAnomaly];
        let score = compute_composite_score(&max_normalized(), &default_weights(), &flags).unwrap();
        // 950_000 - (200_000 / 2) = 950_000 - 100_000 = 850_000
        assert_eq!(score, 850_000);
    }

    #[test]
    fn score_max_metrics_with_two_flags() {
        let flags = vec![
            AntiGamingFlag::ViewVelocityAnomaly,
            AntiGamingFlag::EngagementRatioAnomaly,
        ];
        let score = compute_composite_score(&max_normalized(), &default_weights(), &flags).unwrap();
        // 950_000 - 200_000 = 750_000
        assert_eq!(score, 750_000);
    }

    #[test]
    fn score_half_metrics() {
        let half = NormalizedMetrics {
            views: 500_000,
            likes: 500_000,
            comments: 500_000,
            engagement_rate: 500_000,
            watch_proxy: 500_000,
        };
        let score = compute_composite_score(&half, &default_weights(), &[]).unwrap();
        // Each component is half of its weight: 100_000 + 75_000 + 75_000 + 125_000 + 100_000 = 475_000
        assert_eq!(score, 475_000);
    }

    #[test]
    fn score_clamped_to_max() {
        // Even with unusual weights, score should never exceed MAX_SCORE
        let score = compute_composite_score(&max_normalized(), &default_weights(), &[]).unwrap();
        assert!(score <= MAX_SCORE);
    }

    #[test]
    fn score_penalty_cannot_go_negative() {
        // Small gross score with max penalty
        let low = NormalizedMetrics {
            views: 50_000,
            likes: 50_000,
            comments: 50_000,
            engagement_rate: 50_000,
            watch_proxy: 50_000,
        };
        let flags = vec![
            AntiGamingFlag::ViewVelocityAnomaly,
            AntiGamingFlag::EngagementRatioAnomaly,
        ];
        let score = compute_composite_score(&low, &default_weights(), &flags).unwrap();
        // Gross = 95_000 (5% of 950_000). Penalty = 200_000. saturating_sub => 0
        assert_eq!(score, 0);
    }

    #[test]
    fn invalid_weight_too_low() {
        let mut weights = default_weights();
        weights.views = 10_000; // below MIN_WEIGHT
        let result = compute_composite_score(&max_normalized(), &weights, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_weight_too_high() {
        let mut weights = default_weights();
        weights.views = 600_000; // above MAX_WEIGHT
        let result = compute_composite_score(&max_normalized(), &weights, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn weighted_component_calculation() {
        // 500_000 (half) * 200_000 (0.20) / 1_000_000 = 100_000
        let result = weighted_component(500_000, 200_000).unwrap();
        assert_eq!(result, 100_000);
    }

    #[test]
    fn weighted_component_zero() {
        let result = weighted_component(0, 200_000).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn bot_penalty_no_flags() {
        let result = compute_bot_penalty(200_000, &[]).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn bot_penalty_one_flag() {
        let flags = vec![AntiGamingFlag::ViewVelocityAnomaly];
        let result = compute_bot_penalty(200_000, &flags).unwrap();
        assert_eq!(result, 100_000);
    }

    #[test]
    fn bot_penalty_two_flags() {
        let flags = vec![
            AntiGamingFlag::ViewVelocityAnomaly,
            AntiGamingFlag::EngagementRatioAnomaly,
        ];
        let result = compute_bot_penalty(200_000, &flags).unwrap();
        assert_eq!(result, 200_000);
    }
}
