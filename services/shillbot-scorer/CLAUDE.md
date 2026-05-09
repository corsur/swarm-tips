# Shillbot Scorer — Service Context

Composite scoring and content screening. **Open-sourced from
`coordination-app/backend/shillbot-scorer/` to this public location on
2026-05-02** so the algorithm is publicly auditable (see `README.md`
for the open-source intent).

Read the umbrella `swarm/CLAUDE.md` for shared code standards and
`swarm/shillbot/CLAUDE.md` for the protocol-level scoring-weights /
anti-gaming / brand-safety design context. The pre-extraction service
context (request flow, deployment, consumers in `shillbot-verifier`)
lived in `coordination-app/backend/CLAUDE.md` and remains the canonical
operational doc — this file covers the algorithm only.

---

## Responsibilities

- Compute composite scores from YouTube engagement metrics
- Brand safety content screening (blocklist, topic relevance, duplicate detection, AI disclosure)
- Anti-gaming detection (v1: view velocity, duplicate content)
- Data collection for future anti-gaming analysis
- Read scoring parameters from experiment config (A/B testable)

---

## Scoring Flow

```
Verifier calls scorer with (metrics, task_brief, experiment_config)
        │
        ▼
   Content Screening (pass/fail gate)
   ├── Video description contains task PDA nonce?
   ├── Transcript analysis: blocklisted terms?
   ├── Topic relevance: semantic match to campaign brief?
   ├── Duplicate check: content hash vs. previous submissions?
   ├── AI disclosure label present in video metadata?
   │
   ├── ANY check fails → score = 0, screening_passed = false
   │
   ▼ (all checks pass)
   Composite Score Computation
   ├── Apply weights from experiment_config to each metric
   ├── Compute engagement rate (likes + comments) / views
   ├── Compute watch-through proxy (likes + comments) / views ratio
   ├── Apply bot engagement penalty if anomalies detected
   ├── Sum weighted scores → composite score (0 to MAX_SCORE)
   │
   ▼
   Anti-Gaming Checks
   ├── View velocity: did >30% of views arrive in a single 5-min window?
   ├── Engagement ratio anomaly: likes/views ratio outside normal range?
   ├── If flagged: reduce score or flag for manual review
   │
   ▼
   Return { composite_score, screening_passed, screening_details, flags }
```

---

## Composite Score Formula

```
score = w_views * normalize(views)
      + w_likes * normalize(likes)
      + w_comments * normalize(comments)
      + w_engagement * normalize(engagement_rate)
      + w_watch_proxy * normalize(watch_proxy_ratio)
      - w_penalty * bot_penalty_factor
```

### Default Weights (A/B testable)

| Metric | Weight | Key |
|--------|--------|-----|
| Views | 0.20 | `w_views` |
| Likes | 0.15 | `w_likes` |
| Comments | 0.15 | `w_comments` |
| Engagement rate | 0.25 | `w_engagement` |
| Watch-through proxy | 0.20 | `w_watch_proxy` |
| Bot engagement penalty | -0.20 | `w_penalty` |

Weights are read from the task's experiment config, not hardcoded. The scorer must accept any valid weight configuration (each weight between 0.05 and 0.50, positive weights sum to ~1.0).

### Normalization

Raw metrics are normalized to a 0-1 scale (represented as fixed-point u64 with MAX = 1,000,000) before weighting:
- **Views:** `min(views * 1_000_000 / VIEWS_SCALE, 1_000_000)`
- **Likes:** `min(likes * 1_000_000 / LIKES_SCALE, 1_000_000)`
- **Comments:** `min(comments * 1_000_000 / COMMENTS_SCALE, 1_000_000)`
- **Engagement rate:** `min((likes + comments) * 1_000_000 / max(views, 1), 1_000_000)`
- **Watch-through proxy:** same as engagement rate (v1 — these are the same metric with different weights to emphasize the ratio)

**Default scale factors (A/B testable):**

| Metric | Scale Factor | Meaning |
|--------|-------------|---------|
| VIEWS_SCALE | 5,000 | A Short with 5,000 views scores 1.0 on the views metric |
| LIKES_SCALE | 250 | A Short with 250 likes scores 1.0 |
| COMMENTS_SCALE | 50 | A Short with 50 comments scores 1.0 |

These are starting defaults calibrated from typical YouTube Shorts performance for new channels in the crypto niche. They will be adjusted based on founder-seeded data (Days 1-14). Scale factors are stored in experiment config and read per-task.

**Guard against division by zero:** if `views == 0`, engagement rate and watch-through proxy are both 0 (not undefined). The circuit breaker in the verifier should catch all-zero metrics before they reach the scorer, but the scorer must handle it gracefully regardless.

### Bot Penalty

V1: flag if view velocity is anomalous (>30% of total views in a single 5-minute window, outside the first hour after upload). Apply penalty factor proportional to the anomaly severity.

Future: cross-agent engagement correlation, viewer profile analysis, comment quality scoring.

---

## Content Screening Details

### Task Nonce Verification
- The task has a 16-byte random nonce assigned at creation, emitted in the `TaskCreated` event as hex
- The agent must include the string `[shillbot:{hex_nonce}]` in the YouTube video description (e.g., `[shillbot:a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8]`)
- The scorer verifies this exact pattern exists in the video description via regex: `\[shillbot:[0-9a-f]{32}\]`
- The hex value must match the task's nonce. If missing or mismatched, screening fails.
- This prevents video ID substitution attacks (submitting someone else's viral Short)

### Blocklist Check
- Campaign brief includes `blocklist: { topics: [...], keywords: [...], imagery_terms: [...] }`
- Scorer extracts video transcript (from YouTube captions API or speech-to-text)
- Check transcript against blocklist keywords (case-insensitive, whole-word match)
- Also check video title and description

### Topic Relevance
- Campaign brief includes `topic` (string) and `keywords` (list of strings)
- Scorer tokenizes the video transcript (or title + description if transcript unavailable) into lowercase words
- V1: keyword overlap = `(count of brief keywords found in content) / (total brief keywords)`
- **Pass threshold: 30% keyword overlap.** If the brief has 10 keywords and 3 appear in the transcript, that's 30% — passing. Below 30% = fail screening.
- Matching is case-insensitive, whole-word (word boundary match, not substring — "game" matches "game" but not "gamer")
- If transcript is unavailable AND title + description contain fewer than 10 words total, pass by default (insufficient data to judge — let the quality threshold handle it via engagement metrics)
- Future: LLM-based semantic relevance scoring

### Duplicate Detection
- Compute a content fingerprint: `SHA-256(lowercase(title) || "|" || duration_seconds || "|" || lowercase(first_200_chars_of_transcript_or_description))`
- The `||` is string concatenation. Use `|` as delimiter to prevent ambiguity. Lowercase to normalize.
- If transcript is unavailable, use the first 200 characters of the video description instead.
- Check fingerprint against all previous submissions for the same campaign (Firestore query on `scoring_logs` collection filtered by `campaign_id`)
- Also check cross-campaign for the same agent wallet (agent reusing content across clients)
- Exact match = fail screening. Near-match (future: cosine similarity > 0.9 on transcript embeddings) is deferred to post-v1.

### AI Disclosure
- Check YouTube video metadata for the AI-generated content label
- YouTube requires this for AI-generated content; absence suggests non-compliance

---

## Anti-Gaming Data Collection

Even when v1 anti-gaming is basic (velocity checks), the scorer logs raw data for future analysis:

```
scoring_logs/{task_id}
├── raw_metrics:        { views, likes, comments, at_timestamp }
├── normalized_scores:  per-metric normalized values
├── weights_used:       experiment config weights
├── composite_score:    final score
├── screening_result:   pass/fail with details
├── velocity_data:      view count snapshots at multiple timestamps (if available)
├── engagement_pattern: likes/comments distribution over time
└── flags:              any anti-gaming flags triggered
```

This data feeds the future anti-gaming systems: cross-post correlation, statistical anomaly detection, engager profiling.

---

## API (Library, called by Verifier)

shillbot-scorer ships as a **library crate**, not an HTTP service. The verifier imports it directly (`shillbot_scorer::*`) and calls the public scoring entry point in-process. There is no `POST /score` endpoint and no axum router — earlier docs claimed an HTTP shape, but the architecture settled on direct linkage to avoid an extra hop on every verification.

Function signature (paraphrased):

```rust
pub fn score_submission(
    metrics: EngagementMetrics,
    task_brief: TaskBrief,
    video_metadata: VideoMetadata,
    experiment_config: ExperimentConfig,
) -> ScoringResult {
    /* composite_score, screening_passed, screening_details,
       flags, payment_amount */
}
```

---

## Key Invariants

- Scoring is deterministic: same inputs always produce same score
- All weights come from experiment config, never hardcoded
- Content screening is a hard gate: fail any check = score of 0
- The scorer never accesses on-chain state directly — it receives metrics and config as input
- Anti-gaming flags reduce score or trigger manual review, never silently ignored
