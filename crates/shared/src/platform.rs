use borsh::{BorshDeserialize, BorshSerialize};

use crate::constants::MAX_SCORE;

/// Supported content platforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
pub enum PlatformType {
    YouTube = 0,
    Farcaster = 1,
    TikTok = 2,
}

impl PlatformType {
    /// Converts a raw u8 into a PlatformType, returning None for unknown values.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::YouTube),
            1 => Some(Self::Farcaster),
            2 => Some(Self::TikTok),
            _ => None,
        }
    }
}

/// Platform-agnostic proof of content existence.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PlatformProof {
    pub platform: PlatformType,
    pub content_id_hash: [u8; 32],
    pub nonce: [u8; 16],
    pub timestamp: i64,
}

impl PlatformProof {
    /// Validates that the proof fields are well-formed.
    ///
    /// Checks:
    /// - content_id_hash is not all zeros (a zero hash indicates missing data)
    /// - nonce is not all zeros (a zero nonce indicates missing randomness)
    /// - timestamp is positive (Unix timestamps before epoch are invalid for content)
    pub fn validate(&self) -> Result<(), &'static str> {
        // Precondition: content hash must be non-zero
        if self.content_id_hash == [0u8; 32] {
            return Err("content_id_hash must not be zero");
        }
        // Precondition: nonce must be non-zero
        if self.nonce == [0u8; 16] {
            return Err("nonce must not be zero");
        }
        // Postcondition: timestamp must be positive
        if self.timestamp <= 0 {
            return Err("timestamp must be positive");
        }
        Ok(())
    }
}

/// Platform-agnostic engagement metrics.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EngagementMetrics {
    pub views: u64,
    pub likes: u64,
    pub comments: u64,
    pub shares: u64,
    /// Engagements per view in basis points (engagements / views * 10_000).
    pub engagement_rate_bps: u64,
}

impl EngagementMetrics {
    /// Validates that engagement metrics are internally consistent.
    ///
    /// Checks:
    /// - If views is zero, all other metrics must also be zero
    /// - Individual metrics (likes, comments, shares) do not exceed views
    /// - engagement_rate_bps does not exceed MAX_SCORE (sanity upper bound)
    pub fn validate(&self) -> Result<(), &'static str> {
        // Precondition: zero views implies zero everything
        if self.views == 0 {
            if self.likes != 0 || self.comments != 0 || self.shares != 0 {
                return Err("non-zero metrics with zero views");
            }
            if self.engagement_rate_bps != 0 {
                return Err("non-zero engagement rate with zero views");
            }
            return Ok(());
        }
        // Precondition: individual metrics cannot exceed views
        if self.likes > self.views {
            return Err("likes exceeds views");
        }
        if self.comments > self.views {
            return Err("comments exceeds views");
        }
        if self.shares > self.views {
            return Err("shares exceeds views");
        }
        // Postcondition: engagement rate has a sane upper bound
        if self.engagement_rate_bps > MAX_SCORE {
            return Err("engagement_rate_bps exceeds MAX_SCORE");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- PlatformType tests ---

    #[test]
    fn platform_type_from_u8_valid() {
        assert_eq!(PlatformType::from_u8(0), Some(PlatformType::YouTube));
        assert_eq!(PlatformType::from_u8(1), Some(PlatformType::Farcaster));
        assert_eq!(PlatformType::from_u8(2), Some(PlatformType::TikTok));
    }

    #[test]
    fn platform_type_from_u8_invalid() {
        assert_eq!(PlatformType::from_u8(3), None);
        assert_eq!(PlatformType::from_u8(255), None);
    }

    #[test]
    fn platform_type_roundtrip_serialization() {
        for variant in [
            PlatformType::YouTube,
            PlatformType::Farcaster,
            PlatformType::TikTok,
        ] {
            let bytes = borsh::to_vec(&variant).expect("serialize");
            let deserialized = PlatformType::try_from_slice(&bytes).expect("deserialize");
            assert_eq!(variant, deserialized);
        }
    }

    // --- PlatformProof tests ---

    fn valid_proof() -> PlatformProof {
        PlatformProof {
            platform: PlatformType::YouTube,
            content_id_hash: [0xAB; 32],
            nonce: [0xCD; 16],
            timestamp: 1_700_000_000,
        }
    }

    #[test]
    fn platform_proof_valid() {
        assert!(valid_proof().validate().is_ok());
    }

    #[test]
    fn platform_proof_zero_content_hash_rejected() {
        let mut proof = valid_proof();
        proof.content_id_hash = [0u8; 32];
        assert_eq!(proof.validate(), Err("content_id_hash must not be zero"));
    }

    #[test]
    fn platform_proof_zero_nonce_rejected() {
        let mut proof = valid_proof();
        proof.nonce = [0u8; 16];
        assert_eq!(proof.validate(), Err("nonce must not be zero"));
    }

    #[test]
    fn platform_proof_zero_timestamp_rejected() {
        let mut proof = valid_proof();
        proof.timestamp = 0;
        assert_eq!(proof.validate(), Err("timestamp must be positive"));
    }

    #[test]
    fn platform_proof_negative_timestamp_rejected() {
        let mut proof = valid_proof();
        proof.timestamp = -1;
        assert_eq!(proof.validate(), Err("timestamp must be positive"));
    }

    #[test]
    fn platform_proof_roundtrip_serialization() {
        let proof = valid_proof();
        let bytes = borsh::to_vec(&proof).expect("serialize");
        let deserialized = PlatformProof::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(proof, deserialized);
    }

    // --- EngagementMetrics tests ---

    fn valid_metrics() -> EngagementMetrics {
        EngagementMetrics {
            views: 10_000,
            likes: 500,
            comments: 100,
            shares: 50,
            engagement_rate_bps: 650,
        }
    }

    #[test]
    fn engagement_metrics_valid() {
        assert!(valid_metrics().validate().is_ok());
    }

    #[test]
    fn engagement_metrics_all_zero_valid() {
        let metrics = EngagementMetrics::default();
        assert!(metrics.validate().is_ok());
    }

    #[test]
    fn engagement_metrics_zero_views_nonzero_likes_rejected() {
        let metrics = EngagementMetrics {
            views: 0,
            likes: 1,
            ..Default::default()
        };
        assert_eq!(metrics.validate(), Err("non-zero metrics with zero views"));
    }

    #[test]
    fn engagement_metrics_zero_views_nonzero_comments_rejected() {
        let metrics = EngagementMetrics {
            views: 0,
            comments: 1,
            ..Default::default()
        };
        assert_eq!(metrics.validate(), Err("non-zero metrics with zero views"));
    }

    #[test]
    fn engagement_metrics_zero_views_nonzero_shares_rejected() {
        let metrics = EngagementMetrics {
            views: 0,
            shares: 1,
            ..Default::default()
        };
        assert_eq!(metrics.validate(), Err("non-zero metrics with zero views"));
    }

    #[test]
    fn engagement_metrics_zero_views_nonzero_rate_rejected() {
        let metrics = EngagementMetrics {
            views: 0,
            engagement_rate_bps: 1,
            ..Default::default()
        };
        assert_eq!(
            metrics.validate(),
            Err("non-zero engagement rate with zero views")
        );
    }

    #[test]
    fn engagement_metrics_likes_exceed_views_rejected() {
        let mut metrics = valid_metrics();
        metrics.likes = metrics.views.checked_add(1).unwrap();
        assert_eq!(metrics.validate(), Err("likes exceeds views"));
    }

    #[test]
    fn engagement_metrics_comments_exceed_views_rejected() {
        let mut metrics = valid_metrics();
        metrics.comments = metrics.views.checked_add(1).unwrap();
        assert_eq!(metrics.validate(), Err("comments exceeds views"));
    }

    #[test]
    fn engagement_metrics_shares_exceed_views_rejected() {
        let mut metrics = valid_metrics();
        metrics.shares = metrics.views.checked_add(1).unwrap();
        assert_eq!(metrics.validate(), Err("shares exceeds views"));
    }

    #[test]
    fn engagement_metrics_rate_exceeds_max_score_rejected() {
        let mut metrics = valid_metrics();
        metrics.engagement_rate_bps = MAX_SCORE.checked_add(1).unwrap();
        assert_eq!(
            metrics.validate(),
            Err("engagement_rate_bps exceeds MAX_SCORE")
        );
    }

    #[test]
    fn engagement_metrics_rate_at_max_score_valid() {
        let mut metrics = valid_metrics();
        metrics.engagement_rate_bps = MAX_SCORE;
        assert!(metrics.validate().is_ok());
    }

    #[test]
    fn engagement_metrics_boundary_likes_equal_views_valid() {
        let mut metrics = valid_metrics();
        metrics.likes = metrics.views;
        assert!(metrics.validate().is_ok());
    }

    #[test]
    fn engagement_metrics_roundtrip_serialization() {
        let metrics = valid_metrics();
        let bytes = borsh::to_vec(&metrics).expect("serialize");
        let deserialized = EngagementMetrics::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(metrics, deserialized);
    }
}
