use serde::{Deserialize, Serialize};

/// Maximum composite score value (fixed-point representation of 1.0).
pub const MAX_SCORE: u64 = 1_000_000;

/// Default scale factors for metric normalization.
pub const DEFAULT_VIEWS_SCALE: u64 = 5_000;
pub const DEFAULT_LIKES_SCALE: u64 = 250;
pub const DEFAULT_COMMENTS_SCALE: u64 = 50;

/// Minimum and maximum allowed weight values (fixed-point, 1_000_000 = 1.0).
pub const MIN_WEIGHT: u64 = 50_000; // 0.05
pub const MAX_WEIGHT: u64 = 500_000; // 0.50

/// Raw engagement metrics from YouTube Data API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngagementMetrics {
    pub views: u64,
    pub likes: u64,
    pub comments: u64,
}

/// Weights for composite score computation (fixed-point, MAX_SCORE = 1.0).
/// Each weight is in range [MIN_WEIGHT, MAX_WEIGHT].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringWeights {
    pub views: u64,
    pub likes: u64,
    pub comments: u64,
    pub engagement: u64,
    pub watch_proxy: u64,
    pub bot_penalty: u64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            views: 200_000,       // 0.20
            likes: 150_000,       // 0.15
            comments: 150_000,    // 0.15
            engagement: 250_000,  // 0.25
            watch_proxy: 200_000, // 0.20
            bot_penalty: 200_000, // 0.20
        }
    }
}

/// Scale factors for normalization (configurable per experiment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleFactors {
    pub views: u64,
    pub likes: u64,
    pub comments: u64,
}

impl Default for ScaleFactors {
    fn default() -> Self {
        Self {
            views: DEFAULT_VIEWS_SCALE,
            likes: DEFAULT_LIKES_SCALE,
            comments: DEFAULT_COMMENTS_SCALE,
        }
    }
}

/// Per-cohort scoring strategy.
///
/// Different platforms have fundamentally different success criteria. For
/// engagement platforms (YouTube, X) the protocol pays more for higher-quality
/// work measured by views/likes/comments. For binary platforms (game-play,
/// referral) verification has already proved success against an authoritative
/// source (on-chain Game PDA, Firestore campaign doc), and there is no
/// "quality of doing" — the agent either did the thing or didn't.
///
/// The scoring strategy is data, not code: each cohort doc in Firestore picks
/// one. Adding a new platform later means creating a new cohort doc and
/// choosing its mode, with zero verifier code changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringMode {
    /// Pay full price when screening passes; pay zero when it fails.
    /// Use for binary-completion platforms (game-play, referral).
    Binary,

    /// Pay scaled to composite engagement score above a quality threshold.
    /// Use for engagement platforms (YouTube, X).
    #[default]
    EngagementComposite,
}

/// Experiment configuration passed by the verifier.
///
/// Read from `experiment_configs/{cohort_name}` Firestore docs. The cohort
/// docs are seeded with the orchestrator-style field name `scoring_weights`
/// (because the orchestrator's `ExperimentConfigDoc` reads the same document
/// for client-facing campaign metrics), so we accept both `weights` and
/// `scoring_weights` here. Without the alias, the verifier silently fails
/// to deserialize cohort docs (drops to default), runs the wrong scoring
/// mode, and ignores cohort-specific scale factors — the exact bug behind
/// the Build 6 game-play smoke test pay-out underrun.
///
/// Both `#[serde(default)]` on `weights` and `scale_factors` is also
/// defensive — if a cohort doc is missing either, fall back to the scorer
/// defaults rather than failing the whole deserialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExperimentConfig {
    #[serde(default, alias = "scoring_weights")]
    pub weights: ScoringWeights,

    #[serde(default)]
    pub scale_factors: ScaleFactors,

    /// Scoring strategy for this cohort. Defaults to `EngagementComposite` so
    /// existing cohort docs without the field continue to behave as before.
    #[serde(default)]
    pub scoring_mode: ScoringMode,

    /// Per-cohort verification delay override. `None` means the orchestrator
    /// uses its platform-default fallback table at submit time.
    #[serde(default)]
    pub verification_delay_secs: Option<u64>,
}

/// Campaign brief fields relevant to content screening.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBrief {
    pub topic: String,
    pub keywords: Vec<String>,
    pub blocklist_keywords: Vec<String>,
}

/// Content metadata for content screening.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMetadata {
    pub title: String,
    pub description: String,
    /// Content transcript if available; None if unavailable.
    pub transcript: Option<String>,
    /// The expected nonce hex string (32 hex chars) for the task.
    pub expected_nonce: String,
    /// Duration in seconds. None for platforms without duration (e.g., X threads).
    pub duration_seconds: Option<u64>,
    /// Whether the AI disclosure label is present.
    pub has_ai_disclosure: bool,
}

/// Per-check screening result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreeningDetails {
    pub nonce_valid: bool,
    pub blocklist_clean: bool,
    pub topic_relevant: bool,
    pub not_duplicate: bool,
    pub ai_disclosure_present: bool,
}

/// Normalized metric values (each in range [0, MAX_SCORE]).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NormalizedMetrics {
    pub views: u64,
    pub likes: u64,
    pub comments: u64,
    pub engagement_rate: u64,
    pub watch_proxy: u64,
}

/// Flags raised by anti-gaming checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AntiGamingFlag {
    ViewVelocityAnomaly,
    EngagementRatioAnomaly,
}

/// View count snapshot at a point in time (for velocity analysis).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewSnapshot {
    /// Minutes since upload.
    pub minutes_since_upload: u64,
    pub view_count: u64,
}

/// Full scoring response returned to the verifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringResult {
    pub composite_score: u64,
    pub screening_passed: bool,
    pub screening_details: ScreeningDetails,
    pub normalized_metrics: NormalizedMetrics,
    pub flags: Vec<AntiGamingFlag>,
}
