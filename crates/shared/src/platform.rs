use borsh::{BorshDeserialize, BorshSerialize};

use crate::constants::MAX_SCORE;

/// Supported content platforms.
///
/// Discriminants are stable: they appear on-chain in `Task.platform` and must
/// not be reordered. Append-only; new platforms get the next free value.
///
/// Slot 4 was named `Instagram` historically but the marketplace never
/// shipped Instagram support — every production task with platform=4 is a
/// Shillbot Referral campaign (clients pay agents to seed new shillbot
/// campaigns). The variant was renamed `Referral` to match production
/// reality. The `u8` byte is unchanged; existing on-chain `Task` accounts
/// deserialize identically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
pub enum PlatformType {
    YouTube = 0,
    Farcaster = 1,
    TikTok = 2,
    Twitter = 3,
    Referral = 4,
    GamePlay = 5,
    Reddit = 6,
    Podcast = 7,
    Blog = 8,
    Website = 9,
    /// Deterministic Lean 4 proof check (verification engine vertical,
    /// 2026-07-07). Tasks carry `verification_kind = 1` on-chain and are
    /// verified by the allow-listed attester via `verify_task_attested`,
    /// not the Switchboard oracle. Policy (pinned toolchain, axiom
    /// allow-list, resource caps) = `policies/lean-attester-policy-v1.json`.
    LeanProof = 10,
}

/// How the verifier scores submitted work for a platform.
///
/// Mirrors `shillbot_scorer::models::ScoringMode` but lives in `shared` so
/// the on-chain program and every off-chain consumer (orchestrator, verifier,
/// frontend via JSON, e2e tests) can branch on it without depending on
/// `shillbot-scorer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoringStyle {
    /// Yes/no proof. Score is MAX_SCORE if the platform-specific check passes,
    /// 0 otherwise. Used when the proof is on-chain or HTTP-deterministic
    /// (game-play, referral, website with backlink check).
    Binary,
    /// Engagement metrics from a third-party API, weighted into a composite
    /// score. Used when the platform exposes views/likes/comments and the
    /// score is a continuous function of those.
    EngagementMetrics,
}

impl PlatformType {
    /// Converts a raw u8 into a PlatformType, returning None for unknown values.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::YouTube),
            1 => Some(Self::Farcaster),
            2 => Some(Self::TikTok),
            3 => Some(Self::Twitter),
            4 => Some(Self::Referral),
            5 => Some(Self::GamePlay),
            6 => Some(Self::Reddit),
            7 => Some(Self::Podcast),
            8 => Some(Self::Blog),
            9 => Some(Self::Website),
            10 => Some(Self::LeanProof),
            _ => None,
        }
    }

    /// Discriminant byte. Same as `self as u8` but spelled out for clarity at
    /// call sites that consume raw u8 (frontend over-the-wire JSON, on-chain
    /// task account fields, etc.).
    pub fn discriminant(self) -> u8 {
        self as u8
    }

    /// Human-readable platform name for UI + log output.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::YouTube => "YouTube",
            Self::Farcaster => "Farcaster",
            Self::TikTok => "TikTok",
            Self::Twitter => "X / Twitter",
            Self::Referral => "Shillbot Referral",
            Self::GamePlay => "Coordination Game Play",
            Self::Reddit => "Reddit",
            Self::Podcast => "Podcast",
            Self::Blog => "Blog",
            Self::Website => "Website Footer Backlink",
            Self::LeanProof => "Lean Proof",
        }
    }

    /// Whether shillbot will currently route tasks to this platform. Disabled
    /// platforms appear in the enum (so on-chain bytes stay valid forever)
    /// but the marketplace UI hides them and the verifier rejects new
    /// submissions. Re-enabling = flip this method's match arm + ship the
    /// PlatformClient implementation.
    pub fn is_disabled(self) -> bool {
        match self {
            // No PlatformClient implementations shipped yet.
            Self::Farcaster | Self::TikTok | Self::Reddit | Self::Podcast | Self::Blog => true,
            // YouTube and Twitter implementations exist but their API keys are
            // expired/unavailable in the current production environment. Code
            // paths stay live; campaigns just don't get created.
            Self::YouTube | Self::Twitter => true,
            Self::Referral | Self::GamePlay | Self::Website | Self::LeanProof => false,
        }
    }

    /// Default mainnet verification delay (seconds from `submitted_at` to
    /// when the verifier scores the task). Devnet overrides this uniformly
    /// at the orchestrator boundary — see
    /// `shillbot-orchestrator::models::verification::verification_delay_secs_for_platform_and_network`.
    pub fn default_verification_delay_secs(self) -> u64 {
        match self {
            // Binary platforms: synchronous on-chain or Firestore lookups,
            // 5 minutes lets the cluster propagate the resolving tx. The
            // Lean attester is a deterministic local check with the same
            // propagation needs.
            Self::Referral | Self::GamePlay | Self::LeanProof => 5 * 60,
            // X has no fraud-detection delay built into the API; 2 days
            // catches early engagement spikes from organic discovery before
            // we measure.
            Self::Twitter => 2 * 24 * 60 * 60,
            // Default 7 days: matches YouTube's view-fraud-removal window
            // and gives a sustained-uptime guarantee for footer backlinks.
            Self::YouTube | Self::Website => 7 * 24 * 60 * 60,
            // Disabled platforms — the value doesn't matter today. Use the
            // safe-default 7 days so if one ever ships before this method
            // is updated, it gets the conservative fraud-window delay.
            Self::Farcaster | Self::TikTok | Self::Reddit | Self::Podcast | Self::Blog => {
                7 * 24 * 60 * 60
            }
        }
    }

    /// Default mainnet challenge window (seconds from `verified` to when the
    /// permissionless `finalize_task` crank can run). Binary-mode platforms
    /// score deterministically off a single source, so the challenge window
    /// is short — there's no oracle disagreement to resolve. Metrics-mode
    /// platforms get the full 24 h.
    ///
    /// LeanProof is Binary but keeps a 1-hour window: its payment releases
    /// on a SINGLE attester signature (panel risk: single-attester trust),
    /// so the window must be long enough for a human to react to a
    /// compromised-key attestation — 15 s is not.
    pub fn default_challenge_window_secs(self) -> u64 {
        if self == Self::LeanProof {
            return 60 * 60;
        }
        match self.scoring_style() {
            ScoringStyle::Binary => 15,
            ScoringStyle::EngagementMetrics => 24 * 60 * 60,
        }
    }

    /// How the verifier scores work for this platform. Drives whether
    /// content screening + engagement metrics run (EngagementMetrics) or
    /// the platform client's check is the authoritative proof (Binary).
    pub fn scoring_style(self) -> ScoringStyle {
        match self {
            Self::Referral | Self::GamePlay | Self::Website | Self::LeanProof => {
                ScoringStyle::Binary
            }
            Self::YouTube | Self::Twitter => ScoringStyle::EngagementMetrics,
            Self::Farcaster | Self::TikTok | Self::Reddit | Self::Podcast | Self::Blog => {
                ScoringStyle::EngagementMetrics
            }
        }
    }

    /// Whether AI-disclosure presence can be assumed without an explicit
    /// metadata check. YouTube has an `aiDisclosed` field on the video
    /// resource; the verifier reads it. Other platforms either have no
    /// structured-disclosure metadata at all (X uses an inline `#ad` tag in
    /// the tweet body, treated as implicit) or have no content where
    /// disclosure even applies (referral, game-play, website).
    pub fn has_implicit_ai_disclosure(self) -> bool {
        match self {
            Self::YouTube => false,
            Self::Twitter | Self::Referral | Self::GamePlay | Self::Website | Self::LeanProof => {
                true
            }
            // Conservative default for not-yet-shipped platforms — if you
            // light one up, decide explicitly.
            Self::Farcaster | Self::TikTok | Self::Reddit | Self::Podcast | Self::Blog => false,
        }
    }

    /// Whether the verifier needs to read on-chain data (beyond the
    /// shillbot Task PDA itself) before scoring. Today only game-play
    /// requires this — the verifier reads the coordination-game `Game` PDA
    /// to confirm the agent wallet was a player and the game resolved.
    pub fn needs_on_chain_pre_check(self) -> bool {
        matches!(self, Self::GamePlay)
    }

    /// Whether the campaign-creation form should expose the per-day-spend
    /// cap input. Today only referral campaigns use this — they're
    /// open-ended (one referral begets another) and need a budget brake to
    /// prevent runaway recursion. Other platforms have a fixed task_count
    /// and don't need it.
    pub fn has_daily_cap_field(self) -> bool {
        matches!(self, Self::Referral)
    }

    /// Short hint for the campaign-creation UI describing what the agent's
    /// `content_id` should look like. Used as the form-field placeholder
    /// + validation error message.
    pub fn content_id_label(self) -> &'static str {
        match self {
            Self::YouTube => "YouTube video ID (11 chars)",
            Self::Twitter => "Tweet ID (numeric)",
            Self::Referral => "Campaign UUID",
            Self::GamePlay => "Coordination-game game_id (u64)",
            Self::Website => "URL of the page hosting the swarm.tips backlink",
            Self::Farcaster => "Farcaster cast hash",
            Self::TikTok => "TikTok video ID",
            Self::Reddit => "Reddit post ID",
            Self::Podcast => "Podcast episode URL",
            Self::Blog => "Blog post URL",
            Self::LeanProof => "URL of the Lean proof artifact (Proof.lean, max 5 MB)",
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
        assert_eq!(PlatformType::from_u8(3), Some(PlatformType::Twitter));
        assert_eq!(PlatformType::from_u8(4), Some(PlatformType::Referral));
        assert_eq!(PlatformType::from_u8(5), Some(PlatformType::GamePlay));
        assert_eq!(PlatformType::from_u8(6), Some(PlatformType::Reddit));
        assert_eq!(PlatformType::from_u8(7), Some(PlatformType::Podcast));
        assert_eq!(PlatformType::from_u8(8), Some(PlatformType::Blog));
        assert_eq!(PlatformType::from_u8(9), Some(PlatformType::Website));
        assert_eq!(PlatformType::from_u8(10), Some(PlatformType::LeanProof));
    }

    #[test]
    fn discriminant_matches_repr_u8() {
        assert_eq!(PlatformType::YouTube.discriminant(), 0);
        assert_eq!(PlatformType::Twitter.discriminant(), 3);
        assert_eq!(PlatformType::Referral.discriminant(), 4);
        assert_eq!(PlatformType::GamePlay.discriminant(), 5);
        assert_eq!(PlatformType::Website.discriminant(), 9);
        assert_eq!(PlatformType::LeanProof.discriminant(), 10);
    }

    #[test]
    fn enabled_set_is_exactly_referral_gameplay_website() {
        // Defensive snapshot: this is the surface that ships today.
        // YouTube + Twitter are temporarily disabled (broken API keys);
        // re-enabling means flipping their is_disabled() arm + nothing
        // else (the PlatformClient impls already exist).
        for p in [
            PlatformType::YouTube,
            PlatformType::Farcaster,
            PlatformType::TikTok,
            PlatformType::Twitter,
            PlatformType::Reddit,
            PlatformType::Podcast,
            PlatformType::Blog,
        ] {
            assert!(p.is_disabled(), "{p:?} should be disabled");
        }
        for p in [
            PlatformType::Referral,
            PlatformType::GamePlay,
            PlatformType::Website,
            PlatformType::LeanProof,
        ] {
            assert!(!p.is_disabled(), "{p:?} should be enabled");
        }
    }

    #[test]
    fn binary_scoring_platforms() {
        // The verifier's "force binary scoring" branch was hardcoded for
        // PLATFORM_WEBSITE only; the actual binary set is wider. Pinning
        // it here so a regression in scoring_style() can't silently flip.
        assert_eq!(PlatformType::Referral.scoring_style(), ScoringStyle::Binary);
        assert_eq!(PlatformType::GamePlay.scoring_style(), ScoringStyle::Binary);
        assert_eq!(PlatformType::Website.scoring_style(), ScoringStyle::Binary);
        assert_eq!(
            PlatformType::LeanProof.scoring_style(),
            ScoringStyle::Binary
        );
        assert_eq!(
            PlatformType::YouTube.scoring_style(),
            ScoringStyle::EngagementMetrics
        );
        assert_eq!(
            PlatformType::Twitter.scoring_style(),
            ScoringStyle::EngagementMetrics
        );
    }

    #[test]
    fn youtube_is_only_platform_with_explicit_ai_disclosure() {
        // The verifier had a 4-way OR chain ("Twitter || Referral || GamePlay
        // || Website") that's actually expressing "every shipped platform
        // except YouTube". Pinning it via the inverse so adding a platform
        // that genuinely needs explicit disclosure (e.g., TikTok if it ships)
        // forces a deliberate decision in this method.
        assert!(!PlatformType::YouTube.has_implicit_ai_disclosure());
        for p in [
            PlatformType::Twitter,
            PlatformType::Referral,
            PlatformType::GamePlay,
            PlatformType::Website,
        ] {
            assert!(p.has_implicit_ai_disclosure(), "{p:?}");
        }
    }

    #[test]
    fn only_gameplay_needs_pre_check() {
        // Pinning the special case in pipeline.rs:345.
        for p in [
            PlatformType::YouTube,
            PlatformType::Twitter,
            PlatformType::Referral,
            PlatformType::Website,
            PlatformType::Farcaster,
        ] {
            assert!(!p.needs_on_chain_pre_check(), "{p:?}");
        }
        assert!(PlatformType::GamePlay.needs_on_chain_pre_check());
    }

    #[test]
    fn only_referral_uses_daily_cap_field() {
        for p in [
            PlatformType::YouTube,
            PlatformType::Twitter,
            PlatformType::GamePlay,
            PlatformType::Website,
        ] {
            assert!(!p.has_daily_cap_field(), "{p:?}");
        }
        assert!(PlatformType::Referral.has_daily_cap_field());
    }

    #[test]
    fn binary_platforms_get_short_challenge_window() {
        // 15s for binary, 24h for metrics. Pinned so a scoring_style flip
        // automatically updates the challenge window without manual sync.
        for p in [
            PlatformType::Referral,
            PlatformType::GamePlay,
            PlatformType::Website,
        ] {
            assert_eq!(p.default_challenge_window_secs(), 15, "{p:?}");
        }
        for p in [PlatformType::YouTube, PlatformType::Twitter] {
            assert_eq!(p.default_challenge_window_secs(), 86_400, "{p:?}");
        }
        // LeanProof is Binary but keeps a 1-hour window: payment releases on
        // a SINGLE attester signature, so a human must be able to react to a
        // compromised-key attestation before finalize.
        assert_eq!(
            PlatformType::LeanProof.default_challenge_window_secs(),
            3_600
        );
    }

    #[test]
    fn verification_delays_are_mainnet_safe() {
        // Pin the existing mainnet defaults so this PR doesn't accidentally
        // collapse them into the network-aware devnet path. Devnet override
        // happens at the orchestrator boundary, not here.
        assert_eq!(
            PlatformType::Referral.default_verification_delay_secs(),
            300
        );
        assert_eq!(
            PlatformType::GamePlay.default_verification_delay_secs(),
            300
        );
        assert_eq!(
            PlatformType::LeanProof.default_verification_delay_secs(),
            300
        );
        assert_eq!(
            PlatformType::Twitter.default_verification_delay_secs(),
            172_800
        );
        assert_eq!(
            PlatformType::YouTube.default_verification_delay_secs(),
            604_800
        );
        assert_eq!(
            PlatformType::Website.default_verification_delay_secs(),
            604_800
        );
    }

    #[test]
    fn platform_type_from_u8_invalid() {
        assert_eq!(PlatformType::from_u8(11), None);
        assert_eq!(PlatformType::from_u8(255), None);
    }

    #[test]
    fn platform_type_roundtrip_serialization() {
        for variant in [
            PlatformType::YouTube,
            PlatformType::Farcaster,
            PlatformType::TikTok,
            PlatformType::Twitter,
            PlatformType::Referral,
            PlatformType::GamePlay,
            PlatformType::Reddit,
            PlatformType::Podcast,
            PlatformType::Blog,
            PlatformType::Website,
            PlatformType::LeanProof,
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
