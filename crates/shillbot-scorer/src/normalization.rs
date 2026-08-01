use crate::errors::ScorerError;
use crate::models::{EngagementMetrics, NormalizedMetrics, ScaleFactors, MAX_SCORE};

/// Normalize a raw metric value against a scale factor.
///
/// Returns `min(value * MAX_SCORE / scale, MAX_SCORE)`.
/// If `scale` is zero, returns `Err(ScorerError::InvalidScaleFactor)`.
pub fn normalize_metric(value: u64, scale: u64) -> Result<u64, ScorerError> {
    if scale == 0 {
        return Err(ScorerError::InvalidScaleFactor(
            "scale factor must be non-zero".to_string(),
        ));
    }

    let numerator = value
        .checked_mul(MAX_SCORE)
        .ok_or(ScorerError::ArithmeticOverflow)?;
    let normalized = numerator
        .checked_div(scale)
        .ok_or(ScorerError::ArithmeticOverflow)?;
    let clamped = normalized.min(MAX_SCORE);

    assert!(clamped <= MAX_SCORE, "normalized value exceeds MAX_SCORE");
    Ok(clamped)
}

/// Compute normalized engagement rate: (likes + comments) * MAX_SCORE / max(views, 1).
///
/// If views is zero, returns 0 (not undefined).
/// Clamped to MAX_SCORE.
pub fn normalize_engagement_rate(metrics: &EngagementMetrics) -> Result<u64, ScorerError> {
    const {
        assert!(
            MAX_SCORE > 0,
            "MAX_SCORE must be positive for normalization"
        )
    };

    let engagements = metrics
        .likes
        .checked_add(metrics.comments)
        .ok_or(ScorerError::ArithmeticOverflow)?;

    if metrics.views == 0 {
        return Ok(0);
    }

    let numerator = engagements
        .checked_mul(MAX_SCORE)
        .ok_or(ScorerError::ArithmeticOverflow)?;
    let rate = numerator
        .checked_div(metrics.views)
        .ok_or(ScorerError::ArithmeticOverflow)?;
    let clamped = rate.min(MAX_SCORE);

    assert!(clamped <= MAX_SCORE, "engagement rate exceeds MAX_SCORE");
    Ok(clamped)
}

/// Reject a zero scale factor before it reaches `normalize_metric` as a
/// divisor. `ExperimentConfig` is deserialized from a Firestore cohort doc, so
/// this is external input: it must be rejected, not asserted on.
pub fn validate_scale_factors(scale_factors: &ScaleFactors) -> Result<(), ScorerError> {
    let all = [
        ("views", scale_factors.views),
        ("likes", scale_factors.likes),
        ("comments", scale_factors.comments),
    ];

    for (name, v) in &all {
        if *v == 0 {
            return Err(ScorerError::InvalidScaleFactor(format!(
                "{name} scale factor must be non-zero"
            )));
        }
    }

    Ok(())
}

/// Normalize all metrics into fixed-point values in [0, MAX_SCORE].
pub fn normalize_all(
    metrics: &EngagementMetrics,
    scale_factors: &ScaleFactors,
) -> Result<NormalizedMetrics, ScorerError> {
    // No assert!s on the scale factors here. `normalize_metric` already
    // returns a typed InvalidScaleFactor error for a zero scale, so panicking
    // first both duplicated that check and disagreed with it — and the pair
    // only covered views and likes, silently omitting comments, which is used
    // as a divisor exactly the same way.

    let views = normalize_metric(metrics.views, scale_factors.views)?;
    let likes = normalize_metric(metrics.likes, scale_factors.likes)?;
    let comments = normalize_metric(metrics.comments, scale_factors.comments)?;
    let engagement_rate = normalize_engagement_rate(metrics)?;
    // V1: watch_proxy is identical to engagement_rate (different weight applied in scoring)
    let watch_proxy = engagement_rate;

    let result = NormalizedMetrics {
        views,
        likes,
        comments,
        engagement_rate,
        watch_proxy,
    };

    assert!(
        result.views <= MAX_SCORE,
        "normalized views exceeds MAX_SCORE"
    );
    assert!(
        result.likes <= MAX_SCORE,
        "normalized likes exceeds MAX_SCORE"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_metric_zero_value() {
        let result = normalize_metric(0, 5000).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn normalize_metric_at_scale() {
        let result = normalize_metric(5000, 5000).unwrap();
        assert_eq!(result, MAX_SCORE);
    }

    #[test]
    fn normalize_metric_above_scale_clamps() {
        let result = normalize_metric(10_000, 5000).unwrap();
        assert_eq!(result, MAX_SCORE);
    }

    #[test]
    fn normalize_metric_half_scale() {
        let result = normalize_metric(2500, 5000).unwrap();
        assert_eq!(result, 500_000);
    }

    #[test]
    fn normalize_metric_zero_scale_errors() {
        let result = normalize_metric(100, 0);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_metric_custom_scale() {
        let result = normalize_metric(125, 250).unwrap();
        assert_eq!(result, 500_000);
    }

    #[test]
    fn engagement_rate_zero_views_returns_zero() {
        let metrics = EngagementMetrics {
            views: 0,
            likes: 10,
            comments: 5,
        };
        let result = normalize_engagement_rate(&metrics).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn engagement_rate_normal() {
        // 50 engagements / 1000 views = 0.05 => 50_000 fixed-point
        let metrics = EngagementMetrics {
            views: 1000,
            likes: 40,
            comments: 10,
        };
        let result = normalize_engagement_rate(&metrics).unwrap();
        assert_eq!(result, 50_000);
    }

    #[test]
    fn engagement_rate_clamps_at_max() {
        // More engagements than views: clamps to MAX_SCORE
        let metrics = EngagementMetrics {
            views: 10,
            likes: 100,
            comments: 100,
        };
        let result = normalize_engagement_rate(&metrics).unwrap();
        assert_eq!(result, MAX_SCORE);
    }

    #[test]
    fn normalize_all_with_defaults() {
        let metrics = EngagementMetrics {
            views: 2500,
            likes: 125,
            comments: 25,
        };
        let scale = ScaleFactors::default();
        let norm = normalize_all(&metrics, &scale).unwrap();
        assert_eq!(norm.views, 500_000);
        assert_eq!(norm.likes, 500_000);
        assert_eq!(norm.comments, 500_000);
    }

    #[test]
    fn normalize_metric_large_value_overflows_gracefully() {
        // value * MAX_SCORE would overflow u64 for very large values
        let result = normalize_metric(u64::MAX, 5000);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_metric_large_value_within_scale() {
        // value at scale should give MAX_SCORE even for large scale
        let result = normalize_metric(1_000_000, 1_000_000).unwrap();
        assert_eq!(result, MAX_SCORE);
    }

    #[test]
    fn engagement_rate_large_likes_and_comments_overflow() {
        let metrics = EngagementMetrics {
            views: 1,
            likes: u64::MAX,
            comments: 1,
        };
        // likes + comments overflows
        let result = normalize_engagement_rate(&metrics);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_all_zero_metrics() {
        let metrics = EngagementMetrics {
            views: 0,
            likes: 0,
            comments: 0,
        };
        let scale = ScaleFactors::default();
        let norm = normalize_all(&metrics, &scale).unwrap();
        assert_eq!(norm.views, 0);
        assert_eq!(norm.likes, 0);
        assert_eq!(norm.comments, 0);
        assert_eq!(norm.engagement_rate, 0);
        assert_eq!(norm.watch_proxy, 0);
    }

    /// A zero scale factor from a Firestore cohort doc must be REJECTED, not
    /// panic the verifier — and must be caught before screening, so a failed
    /// screening cannot mask it as a legitimate zero score.
    #[test]
    fn validate_scale_factors_rejects_each_zero() {
        for (name, sf) in [
            (
                "views",
                ScaleFactors {
                    views: 0,
                    likes: 1,
                    comments: 1,
                },
            ),
            (
                "likes",
                ScaleFactors {
                    views: 1,
                    likes: 0,
                    comments: 1,
                },
            ),
            (
                "comments",
                ScaleFactors {
                    views: 1,
                    likes: 1,
                    comments: 0,
                },
            ),
        ] {
            let err = validate_scale_factors(&sf).expect_err("zero must be rejected");
            assert!(
                format!("{err}").contains(name),
                "error should name the zero field {name}, got: {err}"
            );
        }

        assert!(validate_scale_factors(&ScaleFactors {
            views: 1,
            likes: 1,
            comments: 1
        })
        .is_ok());
    }
}
