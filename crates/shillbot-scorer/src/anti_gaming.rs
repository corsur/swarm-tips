use crate::models::{AntiGamingFlag, EngagementMetrics, ViewSnapshot};

/// If >30% of views arrived in any single 5-minute window (outside the first hour), flag it.
const VELOCITY_THRESHOLD_PERCENT: u64 = 30;

/// First hour after upload is excluded from velocity checks (initial burst is normal).
const VELOCITY_EXCLUSION_MINUTES: u64 = 60;

/// Normal engagement ratio range: likes/views.
/// Above 20% is suspicious for most YouTube content.
const MAX_NORMAL_LIKE_RATIO_PERCENT: u64 = 20;

/// Run anti-gaming checks and return any flags triggered.
///
/// Returns an empty vec if no anomalies detected.
pub fn check_anti_gaming(
    metrics: &EngagementMetrics,
    view_snapshots: &[ViewSnapshot],
) -> Vec<AntiGamingFlag> {
    assert!(
        view_snapshots.len() <= 10_000,
        "view_snapshots exceeds reasonable bound"
    );

    let mut flags = Vec::new();

    if detect_view_velocity_anomaly(metrics.views, view_snapshots) {
        tracing::info!(
            total_views = metrics.views,
            "view velocity anomaly detected"
        );
        flags.push(AntiGamingFlag::ViewVelocityAnomaly);
    }

    if detect_engagement_ratio_anomaly(metrics) {
        tracing::info!(
            views = metrics.views,
            likes = metrics.likes,
            "engagement ratio anomaly detected"
        );
        flags.push(AntiGamingFlag::EngagementRatioAnomaly);
    }

    assert!(flags.len() <= 2, "at most 2 anti-gaming flags can fire");

    flags
}

/// Detect if >30% of total views arrived in any single 5-minute window
/// (outside the first hour after upload).
///
/// Snapshots must be sorted by `minutes_since_upload`.
fn detect_view_velocity_anomaly(total_views: u64, snapshots: &[ViewSnapshot]) -> bool {
    if total_views == 0 || snapshots.len() < 2 {
        return false;
    }

    // Filter to snapshots outside the first hour
    let eligible: Vec<&ViewSnapshot> = snapshots
        .iter()
        .filter(|s| s.minutes_since_upload >= VELOCITY_EXCLUSION_MINUTES)
        .collect();

    if eligible.len() < 2 {
        return false;
    }

    // Check consecutive pairs for 5-minute windows
    let max_pairs = eligible.len().saturating_sub(1);
    for i in 0..max_pairs {
        let prev = eligible[i];
        let curr = eligible[i.saturating_add(1)];

        let time_diff = curr
            .minutes_since_upload
            .saturating_sub(prev.minutes_since_upload);
        if time_diff > 5 {
            continue;
        }

        let view_diff = curr.view_count.saturating_sub(prev.view_count);

        // view_diff * 100 / total_views > VELOCITY_THRESHOLD_PERCENT.
        // total_views > 0 is guarded at function entry, so the None arm is
        // unreachable; it degrades to 0 rather than panicking (no-.expect rule).
        let percent = view_diff
            .saturating_mul(100)
            .checked_div(total_views)
            .unwrap_or(0);
        if percent > VELOCITY_THRESHOLD_PERCENT {
            return true;
        }
    }

    false
}

/// Detect if the like/view ratio is outside the normal range.
fn detect_engagement_ratio_anomaly(metrics: &EngagementMetrics) -> bool {
    if metrics.views == 0 {
        return false;
    }

    // metrics.views > 0 is guarded above, so the None arm is unreachable; it
    // degrades to 0 rather than panicking (no-.expect rule).
    let like_percent = metrics
        .likes
        .saturating_mul(100)
        .checked_div(metrics.views)
        .unwrap_or(0);

    like_percent > MAX_NORMAL_LIKE_RATIO_PERCENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_anomaly_with_normal_metrics() {
        let metrics = EngagementMetrics {
            views: 1000,
            likes: 50,
            comments: 10,
        };
        let flags = check_anti_gaming(&metrics, &[]);
        assert!(flags.is_empty());
    }

    #[test]
    fn no_velocity_anomaly_with_no_snapshots() {
        assert!(!detect_view_velocity_anomaly(1000, &[]));
    }

    #[test]
    fn no_velocity_anomaly_with_one_snapshot() {
        let snapshots = vec![ViewSnapshot {
            minutes_since_upload: 120,
            view_count: 500,
        }];
        assert!(!detect_view_velocity_anomaly(1000, &snapshots));
    }

    #[test]
    fn velocity_anomaly_detected() {
        let snapshots = vec![
            ViewSnapshot {
                minutes_since_upload: 60,
                view_count: 100,
            },
            ViewSnapshot {
                minutes_since_upload: 65,
                view_count: 500,
            },
        ];
        // 400 views in 5 minutes = 40% of 1000 total => flagged
        assert!(detect_view_velocity_anomaly(1000, &snapshots));
    }

    #[test]
    fn velocity_within_first_hour_ignored() {
        let snapshots = vec![
            ViewSnapshot {
                minutes_since_upload: 10,
                view_count: 100,
            },
            ViewSnapshot {
                minutes_since_upload: 15,
                view_count: 600,
            },
        ];
        // Both within first hour => not flagged
        assert!(!detect_view_velocity_anomaly(1000, &snapshots));
    }

    #[test]
    fn velocity_just_below_threshold() {
        let snapshots = vec![
            ViewSnapshot {
                minutes_since_upload: 120,
                view_count: 100,
            },
            ViewSnapshot {
                minutes_since_upload: 125,
                view_count: 400,
            },
        ];
        // 300 views / 1000 total = 30% => NOT > 30%, so no flag
        assert!(!detect_view_velocity_anomaly(1000, &snapshots));
    }

    #[test]
    fn velocity_zero_views() {
        let snapshots = vec![
            ViewSnapshot {
                minutes_since_upload: 120,
                view_count: 0,
            },
            ViewSnapshot {
                minutes_since_upload: 125,
                view_count: 0,
            },
        ];
        assert!(!detect_view_velocity_anomaly(0, &snapshots));
    }

    #[test]
    fn engagement_ratio_normal() {
        let metrics = EngagementMetrics {
            views: 1000,
            likes: 100,
            comments: 10,
        };
        assert!(!detect_engagement_ratio_anomaly(&metrics));
    }

    #[test]
    fn engagement_ratio_anomaly() {
        let metrics = EngagementMetrics {
            views: 100,
            likes: 50,
            comments: 5,
        };
        // 50% like ratio => flagged
        assert!(detect_engagement_ratio_anomaly(&metrics));
    }

    #[test]
    fn engagement_ratio_zero_views() {
        let metrics = EngagementMetrics {
            views: 0,
            likes: 10,
            comments: 5,
        };
        assert!(!detect_engagement_ratio_anomaly(&metrics));
    }

    #[test]
    fn engagement_ratio_at_boundary() {
        let metrics = EngagementMetrics {
            views: 100,
            likes: 20,
            comments: 0,
        };
        // 20% exactly => NOT > 20%, so no flag
        assert!(!detect_engagement_ratio_anomaly(&metrics));
    }

    #[test]
    fn full_anti_gaming_both_flags() {
        let metrics = EngagementMetrics {
            views: 1000,
            likes: 500, // 50% ratio => anomaly
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
        let flags = check_anti_gaming(&metrics, &snapshots);
        assert!(flags.contains(&AntiGamingFlag::ViewVelocityAnomaly));
        assert!(flags.contains(&AntiGamingFlag::EngagementRatioAnomaly));
    }
}
