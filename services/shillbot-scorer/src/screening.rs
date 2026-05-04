use sha2::{Digest, Sha256};

use crate::models::{ContentMetadata, ScreeningDetails, TaskBrief};

/// Topic relevance pass threshold: 30% keyword overlap.
const RELEVANCE_THRESHOLD_PERCENT: u64 = 30;

/// Minimum word count in title + description to require topic relevance check.
const MIN_WORDS_FOR_RELEVANCE: usize = 10;

/// Run all content screening checks. Returns overall pass/fail and per-check details.
///
/// Any single check failure means screening fails (score = 0).
pub fn run_screening(
    video: &ContentMetadata,
    brief: &TaskBrief,
    existing_fingerprints: &[String],
) -> ScreeningDetails {
    // If nonce is empty, skip the check — the platform uses a different authorship
    // model (e.g., X verifies author ID instead of embedding a nonce tag).
    let nonce_valid = if video.expected_nonce.is_empty() {
        true // Platform doesn't use nonce (e.g., X uses author ID verification)
    } else if video.expected_nonce.len() != 32 {
        false // Malformed nonce
    } else {
        check_nonce(&video.description, &video.expected_nonce)
    };
    let blocklist_clean = check_blocklist(video, &brief.blocklist_keywords);
    let topic_relevant = check_topic_relevance(video, &brief.keywords);
    let fingerprint = compute_content_fingerprint(video);
    let not_duplicate = !existing_fingerprints.contains(&fingerprint);
    let ai_disclosure_present = video.has_ai_disclosure;

    let details = ScreeningDetails {
        nonce_valid,
        blocklist_clean,
        topic_relevant,
        not_duplicate,
        ai_disclosure_present,
    };

    assert!(
        !details.all_passed()
            || (nonce_valid
                && blocklist_clean
                && topic_relevant
                && not_duplicate
                && ai_disclosure_present),
        "all_passed must agree with individual checks"
    );

    details
}

impl ScreeningDetails {
    /// Returns true only if every individual check passed.
    pub fn all_passed(&self) -> bool {
        self.nonce_valid
            && self.blocklist_clean
            && self.topic_relevant
            && self.not_duplicate
            && self.ai_disclosure_present
    }
}

/// Verify the video description contains the expected `[shillbot:{nonce}]` tag.
fn check_nonce(description: &str, expected_nonce: &str) -> bool {
    let pattern = format!("[shillbot:{}]", expected_nonce);
    if !description.contains(&pattern) {
        return false;
    }
    // Also verify via regex that the format is well-formed.
    // This pattern is a compile-time constant and cannot fail to compile,
    // but we handle the error to satisfy the no-.expect() rule.
    let Ok(re) = regex::Regex::new(r"\[shillbot:[0-9a-f]{32}\]") else {
        return false;
    };
    re.is_match(description)
}

/// Check video title, description, and transcript against blocklist keywords.
/// Case-insensitive, whole-word match.
fn check_blocklist(video: &ContentMetadata, blocklist: &[String]) -> bool {
    if blocklist.is_empty() {
        return true;
    }

    let content = build_searchable_content(video);
    let content_lower = content.to_lowercase();

    for keyword in blocklist {
        let kw_lower = keyword.to_lowercase();
        if contains_whole_word(&content_lower, &kw_lower) {
            tracing::info!(keyword = %keyword, "blocklist keyword found in content");
            return false;
        }
    }
    true
}

/// Check topic relevance using keyword overlap.
/// Pass threshold: 30% of brief keywords found in content (whole-word, case-insensitive).
fn check_topic_relevance(video: &ContentMetadata, keywords: &[String]) -> bool {
    if keywords.is_empty() {
        return true;
    }

    let content = build_searchable_content(video);

    // If transcript unavailable and title + description have fewer than MIN_WORDS_FOR_RELEVANCE,
    // pass by default (insufficient data to judge).
    if video.transcript.is_none() {
        let word_count = video
            .title
            .split_whitespace()
            .chain(video.description.split_whitespace())
            .count();
        if word_count < MIN_WORDS_FOR_RELEVANCE {
            return true;
        }
    }

    let content_lower = content.to_lowercase();

    let mut found: u64 = 0;
    for kw in keywords {
        let kw_lower = kw.to_lowercase();
        if contains_whole_word(&content_lower, &kw_lower) {
            // Saturating add is fine here; keyword count is bounded by brief size
            found = found.saturating_add(1);
        }
    }

    let total = keywords.len() as u64;
    // overlap_percent = found * 100 / total
    // Use saturating arithmetic since these are small bounded values
    let overlap_percent = found.saturating_mul(100).checked_div(total).unwrap_or(0);

    overlap_percent >= RELEVANCE_THRESHOLD_PERCENT
}

/// Compute a content fingerprint for duplicate detection.
///
/// `SHA-256(lowercase(title) || "|" || duration_seconds || "|" || lowercase(first_200_chars))`
///
/// Uses transcript if available, otherwise video description.
pub fn compute_content_fingerprint(video: &ContentMetadata) -> String {
    // For X tweets, title is empty — use description as the primary text.
    let title_lower = if video.title.is_empty() {
        video.description.to_lowercase()
    } else {
        video.title.to_lowercase()
    };
    let source_text = video.transcript.as_deref().unwrap_or(&video.description);
    let source_lower = source_text.to_lowercase();

    let first_200: String = source_lower.chars().take(200).collect();

    let duration = video.duration_seconds.unwrap_or(0);
    let input = format!("{}|{}|{}", title_lower, duration, first_200);

    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();
    let result = hex::encode(hash);

    assert_eq!(
        result.len(),
        64,
        "SHA-256 hex fingerprint must be 64 characters"
    );

    result
}

/// Check if `haystack` contains `needle` as a whole word.
/// Word boundaries are non-alphanumeric characters or string start/end.
fn contains_whole_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }

    let pattern = format!(r"\b{}\b", regex::escape(needle));
    let re = match regex::Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => {
            // Fallback: if word-boundary regex fails, try plain escaped match
            match regex::Regex::new(&regex::escape(needle)) {
                Ok(r) => r,
                Err(_) => return false,
            }
        }
    };
    re.is_match(haystack)
}

/// Combine title, description, and transcript (if available) into a single searchable string.
fn build_searchable_content(video: &ContentMetadata) -> String {
    let mut content = format!("{} {}", video.title, video.description);
    if let Some(transcript) = &video.transcript {
        content.push(' ');
        content.push_str(transcript);
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ContentMetadata;

    fn make_video(description: &str) -> ContentMetadata {
        ContentMetadata {
            title: "Test Video Title".to_string(),
            description: description.to_string(),
            transcript: Some("This is a transcript about blockchain and crypto gaming".to_string()),
            expected_nonce: "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8".to_string(),
            duration_seconds: Some(45),
            has_ai_disclosure: true,
        }
    }

    fn make_brief() -> TaskBrief {
        TaskBrief {
            topic: "crypto gaming".to_string(),
            keywords: vec![
                "blockchain".to_string(),
                "crypto".to_string(),
                "gaming".to_string(),
                "solana".to_string(),
                "defi".to_string(),
            ],
            blocklist_keywords: vec!["scam".to_string(), "ponzi".to_string()],
        }
    }

    // --- Nonce checks ---

    #[test]
    fn nonce_valid_present() {
        assert!(check_nonce(
            "Check this out [shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8] cool",
            "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8"
        ));
    }

    #[test]
    fn nonce_missing() {
        assert!(!check_nonce(
            "No nonce here",
            "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8"
        ));
    }

    #[test]
    fn nonce_wrong_value() {
        assert!(!check_nonce(
            "[shillbot:00000000000000000000000000000000]",
            "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8"
        ));
    }

    #[test]
    fn nonce_short_hex() {
        // 31 hex chars instead of 32 -- should not match regex
        assert!(!check_nonce(
            "[shillbot:0000000000000000000000000000000]",
            "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8"
        ));
    }

    #[test]
    fn nonce_malformed_hex() {
        // Contains uppercase — regex requires lowercase hex
        assert!(!check_nonce(
            "[shillbot:A1B2C3D4E5F6A7B8A1B2C3D4E5F6A7B8]",
            "A1B2C3D4E5F6A7B8A1B2C3D4E5F6A7B8"
        ));
    }

    // --- Blocklist checks ---

    #[test]
    fn blocklist_clean_content() {
        let video = make_video("Clean description [shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8]");
        assert!(check_blocklist(
            &video,
            &["scam".to_string(), "ponzi".to_string()]
        ));
    }

    #[test]
    fn blocklist_keyword_in_title() {
        let mut video = make_video("Clean description");
        video.title = "This is a scam project".to_string();
        assert!(!check_blocklist(&video, &["scam".to_string()]));
    }

    #[test]
    fn blocklist_keyword_in_transcript() {
        let mut video = make_video("Clean description");
        video.transcript = Some("This project is a total ponzi scheme".to_string());
        assert!(!check_blocklist(&video, &["ponzi".to_string()]));
    }

    #[test]
    fn blocklist_whole_word_only() {
        let mut video = make_video("Clean description");
        video.transcript = Some("The gameplay is fantastic".to_string());
        // "game" should NOT match "gameplay" (whole-word match)
        assert!(check_blocklist(&video, &["game".to_string()]));
    }

    #[test]
    fn blocklist_case_insensitive() {
        let mut video = make_video("Clean description");
        video.transcript = Some("This is a SCAM".to_string());
        assert!(!check_blocklist(&video, &["scam".to_string()]));
    }

    #[test]
    fn blocklist_empty_passes() {
        let video = make_video("anything");
        assert!(check_blocklist(&video, &[]));
    }

    #[test]
    fn blocklist_special_chars_in_keyword_detected() {
        let mut video = make_video("Clean description");
        video.transcript = Some("Price went up 100% and boom happened".to_string());
        // "boom" as a word should be detected
        assert!(!check_blocklist(&video, &["boom".to_string()]));
    }

    #[test]
    fn blocklist_keyword_with_regex_metachar_does_not_match_similar() {
        let mut video = make_video("Clean description");
        video.transcript = Some("Use this hack to win".to_string());
        // "h.ck" with escaped regex should NOT match "hack" — only literal "h.ck"
        assert!(check_blocklist(&video, &["h.ck".to_string()]));
    }

    #[test]
    fn blocklist_keyword_with_regex_metachar_matches_literal() {
        let mut video = make_video("Clean description");
        video.transcript = Some("The value is h.ck right now".to_string());
        // "h.ck" should match the literal "h.ck" in the transcript
        assert!(!check_blocklist(&video, &["h.ck".to_string()]));
    }

    #[test]
    fn blocklist_empty_keyword_is_safe() {
        let video = make_video("Any content here");
        // Empty keyword in blocklist should not cause a false positive
        assert!(check_blocklist(&video, &["".to_string()]));
    }

    #[test]
    fn nonce_with_uppercase_hex_rejected() {
        // Uppercase hex in the description should fail since regex requires lowercase
        assert!(!check_nonce(
            "[shillbot:A1B2C3D4E5F6A7B8A1B2C3D4E5F6A7B8]",
            "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8"
        ));
    }

    #[test]
    fn nonce_extra_chars_around_tag() {
        // Nonce tag embedded in other text should still match
        assert!(check_nonce(
            "prefix[shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8]suffix",
            "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8"
        ));
    }

    #[test]
    fn nonce_multiple_tags_correct_one_present() {
        // Description has two nonce tags; one is correct
        assert!(check_nonce(
            "[shillbot:00000000000000000000000000000000] [shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8]",
            "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8"
        ));
    }

    // --- Topic relevance checks ---

    #[test]
    fn topic_relevance_above_threshold() {
        let video = make_video("description");
        // transcript: "blockchain and crypto gaming" => contains blockchain, crypto, gaming = 3/5 = 60%
        let keywords = vec![
            "blockchain".to_string(),
            "crypto".to_string(),
            "gaming".to_string(),
            "solana".to_string(),
            "defi".to_string(),
        ];
        assert!(check_topic_relevance(&video, &keywords));
    }

    #[test]
    fn topic_relevance_exactly_at_threshold() {
        // Need exactly 30%: 3 out of 10 keywords
        let mut video = make_video("description");
        video.transcript = Some("This video covers blockchain and crypto topics".to_string());
        let keywords: Vec<String> = vec![
            "blockchain",
            "crypto",
            "topics",
            "solana",
            "defi",
            "nft",
            "token",
            "staking",
            "yield",
            "swap",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        // "blockchain", "crypto", "topics" found = 3/10 = 30% => pass
        assert!(check_topic_relevance(&video, &keywords));
    }

    #[test]
    fn topic_relevance_below_threshold() {
        let mut video = make_video("description");
        video.transcript = Some("This video is about cats and dogs".to_string());
        let keywords: Vec<String> = vec![
            "blockchain",
            "crypto",
            "gaming",
            "solana",
            "defi",
            "nft",
            "token",
            "staking",
            "yield",
            "swap",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        // 0/10 = 0% => fail
        assert!(!check_topic_relevance(&video, &keywords));
    }

    #[test]
    fn topic_relevance_no_transcript_few_words_passes() {
        let mut video = make_video("short desc");
        video.transcript = None;
        video.title = "Hi".to_string();
        video.description = "short".to_string();
        // title + description < 10 words => pass by default
        let keywords = vec!["blockchain".to_string()];
        assert!(check_topic_relevance(&video, &keywords));
    }

    #[test]
    fn topic_relevance_empty_keywords_passes() {
        let video = make_video("anything");
        assert!(check_topic_relevance(&video, &[]));
    }

    // --- Duplicate detection ---

    #[test]
    fn fingerprint_deterministic() {
        let video = make_video("Test description [shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8]");
        let fp1 = compute_content_fingerprint(&video);
        let fp2 = compute_content_fingerprint(&video);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn fingerprint_different_for_different_titles() {
        let mut v1 = make_video("desc");
        v1.title = "Title A".to_string();
        let mut v2 = make_video("desc");
        v2.title = "Title B".to_string();
        assert_ne!(
            compute_content_fingerprint(&v1),
            compute_content_fingerprint(&v2)
        );
    }

    #[test]
    fn fingerprint_different_for_different_durations() {
        let mut v1 = make_video("desc");
        v1.duration_seconds = Some(30);
        let mut v2 = make_video("desc");
        v2.duration_seconds = Some(45);
        assert_ne!(
            compute_content_fingerprint(&v1),
            compute_content_fingerprint(&v2)
        );
    }

    #[test]
    fn fingerprint_uses_transcript_over_description() {
        let mut v1 = make_video("different description");
        v1.transcript = Some("same transcript content".to_string());
        let mut v2 = make_video("another description");
        v2.transcript = Some("same transcript content".to_string());
        v2.title = v1.title.clone();
        v2.duration_seconds = v1.duration_seconds;
        assert_eq!(
            compute_content_fingerprint(&v1),
            compute_content_fingerprint(&v2)
        );
    }

    #[test]
    fn fingerprint_falls_back_to_description() {
        let mut video = make_video("the actual description for fingerprint");
        video.transcript = None;
        let fp = compute_content_fingerprint(&video);
        // Should be a valid hex-encoded SHA-256 (64 chars)
        assert_eq!(fp.len(), 64);
    }

    #[test]
    fn fingerprint_case_insensitive() {
        let mut v1 = make_video("desc");
        v1.title = "ALL CAPS TITLE".to_string();
        let mut v2 = make_video("desc");
        v2.title = "all caps title".to_string();
        v2.transcript = v1.transcript.clone();
        v2.duration_seconds = v1.duration_seconds;
        assert_eq!(
            compute_content_fingerprint(&v1),
            compute_content_fingerprint(&v2)
        );
    }

    // --- Empty nonce (X/Twitter platform) ---

    #[test]
    fn screening_passes_with_empty_nonce() {
        let video = ContentMetadata {
            title: "Great blockchain crypto gaming video".to_string(),
            description: "No nonce tag needed for X platform".to_string(),
            transcript: Some("blockchain crypto gaming is the future of solana".to_string()),
            expected_nonce: String::new(),
            duration_seconds: Some(45),
            has_ai_disclosure: true,
        };
        let brief = make_brief();
        let result = run_screening(&video, &brief, &[]);
        assert!(
            result.nonce_valid,
            "empty nonce should pass (platform uses author ID verification)"
        );
        assert!(result.all_passed());
    }

    // --- Full screening ---

    #[test]
    fn screening_all_pass() {
        let video = ContentMetadata {
            title: "Great blockchain crypto gaming video".to_string(),
            description: "Check out this [shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8] gaming"
                .to_string(),
            transcript: Some("blockchain crypto gaming is the future of solana".to_string()),
            expected_nonce: "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8".to_string(),
            duration_seconds: Some(45),
            has_ai_disclosure: true,
        };
        let brief = make_brief();
        let result = run_screening(&video, &brief, &[]);
        assert!(result.all_passed());
    }

    #[test]
    fn screening_fails_nonce() {
        let video = ContentMetadata {
            title: "Great blockchain crypto gaming video".to_string(),
            description: "No nonce here".to_string(),
            transcript: Some("blockchain crypto gaming is the future".to_string()),
            expected_nonce: "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8".to_string(),
            duration_seconds: Some(45),
            has_ai_disclosure: true,
        };
        let brief = make_brief();
        let result = run_screening(&video, &brief, &[]);
        assert!(!result.nonce_valid);
        assert!(!result.all_passed());
    }

    #[test]
    fn screening_fails_duplicate() {
        let video = ContentMetadata {
            title: "Great blockchain crypto gaming video".to_string(),
            description: "Check out this [shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8] gaming"
                .to_string(),
            transcript: Some("blockchain crypto gaming is great".to_string()),
            expected_nonce: "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8".to_string(),
            duration_seconds: Some(45),
            has_ai_disclosure: true,
        };
        let fp = compute_content_fingerprint(&video);
        let brief = make_brief();
        let result = run_screening(&video, &brief, &[fp]);
        assert!(!result.not_duplicate);
        assert!(!result.all_passed());
    }

    #[test]
    fn screening_fails_ai_disclosure() {
        let video = ContentMetadata {
            title: "Great blockchain crypto gaming video".to_string(),
            description: "Check out [shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8]".to_string(),
            transcript: Some("blockchain crypto gaming is great".to_string()),
            expected_nonce: "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8".to_string(),
            duration_seconds: Some(45),
            has_ai_disclosure: false,
        };
        let brief = make_brief();
        let result = run_screening(&video, &brief, &[]);
        assert!(!result.ai_disclosure_present);
        assert!(!result.all_passed());
    }
}
