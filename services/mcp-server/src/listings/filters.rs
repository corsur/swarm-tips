use crate::listings::models::{IngestionConfig, RawListing};

/// Result of applying quality filters to a listing.
pub struct FilterResult {
    pub filtered: bool,
    pub reason: Option<String>,
}

/// Apply uniform quality filters to a raw listing.
/// Returns whether the listing should be filtered out and why.
pub fn apply_filters(listing: &RawListing, config: &IngestionConfig) -> FilterResult {
    // Check reward: must have a USD estimate above minimum
    let usd_estimate = listing.reward_usd_estimate.unwrap_or(0.0);
    if usd_estimate < config.min_reward_usd {
        return FilterResult {
            filtered: true,
            reason: Some(format!(
                "reward_below_minimum: ${:.2} < ${:.2}",
                usd_estimate, config.min_reward_usd
            )),
        };
    }

    // Check description length
    let desc_len = listing.description.trim().len();
    if desc_len < config.min_description_length {
        return FilterResult {
            filtered: true,
            reason: Some(format!(
                "description_too_short: {} chars < {} minimum",
                desc_len, config.min_description_length
            )),
        };
    }

    FilterResult {
        filtered: false,
        reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_listing(reward_usd: Option<f64>, description: &str) -> RawListing {
        RawListing {
            source: "test".to_string(),
            source_id: "1".to_string(),
            source_url: "https://example.com".to_string(),
            title: "Test".to_string(),
            description: description.to_string(),
            category: "code".to_string(),
            tags: vec![],
            reward_amount: "10".to_string(),
            reward_token: "USDC".to_string(),
            reward_chain: "base".to_string(),
            reward_usd_estimate: reward_usd,
            payment_model: "fixed".to_string(),
            escrow: true,
            posted_at: Utc::now(),
            deadline: None,
        }
    }

    #[test]
    fn passes_with_sufficient_reward_and_description() {
        let config = IngestionConfig::default();
        let listing = make_listing(Some(5.0), "A long enough description for this test case");
        let result = apply_filters(&listing, &config);
        assert!(!result.filtered);
        assert!(result.reason.is_none());
    }

    #[test]
    fn filters_zero_reward() {
        let config = IngestionConfig::default();
        let listing = make_listing(Some(0.0), "A long enough description for this test case");
        let result = apply_filters(&listing, &config);
        assert!(result.filtered);
        assert!(result.reason.unwrap().contains("reward_below_minimum"));
    }

    #[test]
    fn filters_null_reward() {
        let config = IngestionConfig::default();
        let listing = make_listing(None, "A long enough description for this test case");
        let result = apply_filters(&listing, &config);
        assert!(result.filtered);
        assert!(result.reason.unwrap().contains("reward_below_minimum"));
    }

    #[test]
    fn filters_short_description() {
        let config = IngestionConfig::default();
        let listing = make_listing(Some(10.0), "too short");
        let result = apply_filters(&listing, &config);
        assert!(result.filtered);
        assert!(result.reason.unwrap().contains("description_too_short"));
    }

    #[test]
    fn reward_exactly_at_threshold_passes() {
        let config = IngestionConfig::default(); // min_reward_usd = 1.0
        let listing = make_listing(Some(1.0), "A long enough description for this test case");
        let result = apply_filters(&listing, &config);
        assert!(!result.filtered);
    }

    #[test]
    fn reward_just_below_threshold_filtered() {
        let config = IngestionConfig::default();
        let listing = make_listing(Some(0.99), "A long enough description for this test case");
        let result = apply_filters(&listing, &config);
        assert!(result.filtered);
    }
}
