#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]

pub mod anti_gaming;
pub mod errors;
pub mod models;
pub mod normalization;
pub mod scoring;
pub mod screening;

use errors::ScorerError;
use models::{
    ContentMetadata, EngagementMetrics, ExperimentConfig, ScoringResult, TaskBrief, ViewSnapshot,
};

/// Compute the full scoring result: content screening, normalization, scoring, anti-gaming.
///
/// This is the top-level entry point called by the verifier service.
/// Screening is a hard gate: if any check fails, composite_score = 0.
pub fn score_submission(
    metrics: &EngagementMetrics,
    video: &ContentMetadata,
    brief: &TaskBrief,
    config: &ExperimentConfig,
    existing_fingerprints: &[String],
    view_snapshots: &[ViewSnapshot],
) -> Result<ScoringResult, ScorerError> {
    assert!(
        config.scale_factors.views > 0,
        "views scale must be positive"
    );
    assert!(
        config.scale_factors.likes > 0,
        "likes scale must be positive"
    );

    // Weight-range validation happens in compute_composite_score
    // (scoring::validate_weights) against the canonical MIN_WEIGHT/MAX_WEIGHT
    // constants — no duplicated literals here.

    let screening_details = screening::run_screening(video, brief, existing_fingerprints);

    if !screening_details.all_passed() {
        tracing::info!(
            nonce = screening_details.nonce_valid,
            blocklist = screening_details.blocklist_clean,
            relevance = screening_details.topic_relevant,
            duplicate = screening_details.not_duplicate,
            ai_label = screening_details.ai_disclosure_present,
            "content screening failed — score set to 0"
        );

        let zero_normalized = models::NormalizedMetrics {
            views: 0,
            likes: 0,
            comments: 0,
            engagement_rate: 0,
            watch_proxy: 0,
        };

        return Ok(ScoringResult {
            composite_score: 0,
            screening_passed: false,
            screening_details,
            normalized_metrics: zero_normalized,
            flags: Vec::new(),
        });
    }

    let normalized = normalization::normalize_all(metrics, &config.scale_factors)?;
    let flags = anti_gaming::check_anti_gaming(metrics, view_snapshots);
    let composite_score = scoring::compute_composite_score(&normalized, &config.weights, &flags)?;

    tracing::info!(
        composite_score = composite_score,
        flags_count = flags.len(),
        "scoring complete"
    );

    let result = ScoringResult {
        composite_score,
        screening_passed: true,
        screening_details,
        normalized_metrics: normalized,
        flags,
    };

    assert!(
        result.composite_score <= models::MAX_SCORE,
        "final score exceeds MAX_SCORE"
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use models::{ExperimentConfig, ScaleFactors, ScoringWeights};

    fn valid_video() -> ContentMetadata {
        ContentMetadata {
            title: "Amazing blockchain crypto gaming video".to_string(),
            description:
                "Check this out [shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8] great gaming content"
                    .to_string(),
            transcript: Some("blockchain crypto gaming is the future of solana defi".to_string()),
            expected_nonce: "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8".to_string(),
            duration_seconds: Some(45),
            has_ai_disclosure: true,
        }
    }

    fn valid_brief() -> TaskBrief {
        TaskBrief {
            topic: "crypto gaming".to_string(),
            keywords: vec![
                "blockchain".to_string(),
                "crypto".to_string(),
                "gaming".to_string(),
                "solana".to_string(),
                "defi".to_string(),
            ],
            blocklist_keywords: vec!["scam".to_string()],
        }
    }

    #[test]
    fn full_scoring_happy_path() {
        let metrics = EngagementMetrics {
            views: 2500,
            likes: 125,
            comments: 25,
        };
        let config = ExperimentConfig::default();
        let result =
            score_submission(&metrics, &valid_video(), &valid_brief(), &config, &[], &[]).unwrap();

        assert!(result.screening_passed);
        assert!(result.composite_score > 0);
        assert!(result.composite_score <= models::MAX_SCORE);
        assert!(result.flags.is_empty());
    }

    #[test]
    fn full_scoring_screening_failure_returns_zero() {
        let metrics = EngagementMetrics {
            views: 5000,
            likes: 250,
            comments: 50,
        };
        let mut video = valid_video();
        video.has_ai_disclosure = false;
        let config = ExperimentConfig::default();

        let result = score_submission(&metrics, &video, &valid_brief(), &config, &[], &[]).unwrap();

        assert!(!result.screening_passed);
        assert_eq!(result.composite_score, 0);
    }

    #[test]
    fn full_scoring_zero_metrics() {
        let metrics = EngagementMetrics {
            views: 0,
            likes: 0,
            comments: 0,
        };
        let config = ExperimentConfig::default();
        let result =
            score_submission(&metrics, &valid_video(), &valid_brief(), &config, &[], &[]).unwrap();

        assert!(result.screening_passed);
        assert_eq!(result.composite_score, 0);
    }

    #[test]
    fn full_scoring_with_custom_config() {
        let metrics = EngagementMetrics {
            views: 5000,
            likes: 250,
            comments: 50,
        };
        let config = ExperimentConfig {
            weights: ScoringWeights {
                views: 100_000,
                likes: 100_000,
                comments: 100_000,
                engagement: 350_000,
                watch_proxy: 300_000,
                bot_penalty: 100_000,
            },
            scale_factors: ScaleFactors {
                views: 10_000,
                likes: 500,
                comments: 100,
            },
            scoring_mode: Default::default(),
            verification_delay_secs: None,
        };
        let result =
            score_submission(&metrics, &valid_video(), &valid_brief(), &config, &[], &[]).unwrap();

        assert!(result.screening_passed);
        assert!(result.composite_score > 0);
        assert!(result.composite_score <= models::MAX_SCORE);
    }

    #[test]
    fn full_scoring_with_anti_gaming_flags() {
        let metrics = EngagementMetrics {
            views: 1000,
            likes: 500, // 50% like ratio => anomaly
            comments: 10,
        };
        let snapshots = vec![
            ViewSnapshot {
                minutes_since_upload: 120,
                view_count: 100,
            },
            ViewSnapshot {
                minutes_since_upload: 125,
                view_count: 600,
            },
        ];
        let config = ExperimentConfig::default();
        let result = score_submission(
            &metrics,
            &valid_video(),
            &valid_brief(),
            &config,
            &[],
            &snapshots,
        )
        .unwrap();

        assert!(result.screening_passed);
        assert!(!result.flags.is_empty());
    }

    #[test]
    fn full_scoring_max_metrics_clamps() {
        let metrics = EngagementMetrics {
            views: u64::MAX,
            likes: u64::MAX,
            comments: u64::MAX,
        };
        let config = ExperimentConfig::default();
        // Should return ArithmeticOverflow because normalization of u64::MAX overflows
        let result = score_submission(&metrics, &valid_video(), &valid_brief(), &config, &[], &[]);
        assert!(result.is_err());
    }

    #[test]
    fn deterministic_scoring() {
        let metrics = EngagementMetrics {
            views: 3000,
            likes: 150,
            comments: 30,
        };
        let config = ExperimentConfig::default();

        let r1 =
            score_submission(&metrics, &valid_video(), &valid_brief(), &config, &[], &[]).unwrap();
        let r2 =
            score_submission(&metrics, &valid_video(), &valid_brief(), &config, &[], &[]).unwrap();

        assert_eq!(r1.composite_score, r2.composite_score);
    }
}
