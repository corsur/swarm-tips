//! Composite trust scoring (Phase 2 reputation model entry point).
//!
//! Combines multiple per-signal sources into a single 0..1 score:
//!
//! - **Shillbot reputation** — average composite score (already 0..1
//!   normalized via fixed-point) and completion rate (work-quality
//!   signal). High weight when present.
//! - **Coordination Game reputation** — win rate over total_games
//!   (gameplay-skill signal). Moderate weight; orthogonal to
//!   work-output signal.
//! - **Layer 3 curator vetting** — first-party → 1.0, vetted → 0.7,
//!   discovered → 0.3. Reflects DAO-side trust ascription.
//! - **AgentRank** (when available) — code-quality signal for MCP
//!   servers. Already 0..1 normalized by Hyperspace's algorithm.
//!
//! **Partial-data tolerant.** Every input is optional. Missing
//! signals don't penalize; weights renormalize over the present
//! signals only. The output `confidence` field reports how many
//! signals contributed (0..=4) so a caller can decide whether to
//! trust the composite or wait for more inputs.
//!
//! **Composes with EigenTrust.** EigenTrust (the global settlement
//! trust graph, `crates/reputation-indexer`) feeds in as the
//! `eigentrust` signal — rank-normalized graph position from
//! Firestore `agent_reputation/{wallet}`, recomputed event-driven
//! on settlement.

use serde::{Deserialize, Serialize};

/// Per-signal weights when ALL five signals are present. Renormalized
/// when any are missing (the absolute values are relative — only their
/// ratios matter). Tuning rationale:
///
/// - Shillbot 0.45 — biggest single weight because it's the only
///   signal grounded in oracle-attested verified work.
/// - AgentRank 0.25 — strong code-quality signal but indirect:
///   measures the MCP server's maintenance state, not the agent's
///   work quality.
/// - Game 0.15 — direct skill signal but narrow domain (1v1 social
///   deduction); doesn't generalize to content-creation work.
/// - Curator 0.15 — DAO-side ascription. Smaller weight because it's
///   a categorical bin (first-party / vetted / discovered) rather
///   than a fine-grained metric.
/// - Credit-web 0.20 — non-financial reputation: the agent's position
///   in the extension-credit web (who vouched for it, weighted by the
///   voucher's own standing). The novel positive-reputation signal
///   (mund-creanc-witer); B4.
const WEIGHT_SHILLBOT: f64 = 0.45;
/// EigenTrust 0.35 — relational trust over on-chain settled work (who
/// transacted with whom, anchored at first-party wallets). Second-highest
/// weight: settlement-grounded like Shillbot but global/relational rather
/// than local work quality — the two are deliberately orthogonal (the
/// same settlement feeds the composite ONCE as quality, and the graph as
/// an edge).
const WEIGHT_EIGENTRUST: f64 = 0.35;
const WEIGHT_AGENT_RANK: f64 = 0.25;
const WEIGHT_GAME: f64 = 0.15;
const WEIGHT_CURATOR: f64 = 0.15;
const WEIGHT_CREDIT_WEB: f64 = 0.20;

/// All five input signals to the composite trust formula. Each is
/// `Option<...>`; missing signals don't contribute and don't
/// penalize. Populate from:
/// - `shillbot`: `agent_profile.shillbot.derived` (#29 tool)
/// - `game`: `agent_profile.coordination_game.derived` (#29 tool)
/// - `curator`: Layer 3 directory tier (#25, #30)
/// - `agent_rank`: Hyperspace AgentRank API (deferred — pass `None`
///   until that integration ships)
/// - `credit_web`: extension-credit web-position (B4); `None` until
///   the web-position indexer ships.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TrustInputs {
    pub shillbot: Option<ShillbotInput>,
    /// EigenTrust graph position, rank-normalized to 0..1 by the
    /// reputation indexer (`AgentReputation.rank_normalized`). Read from
    /// Firestore `agent_reputation/{wallet}`; absent until the wallet
    /// appears in the settlement graph.
    pub eigentrust: Option<f64>,
    pub game: Option<GameInput>,
    pub curator: Option<CuratorTier>,
    pub agent_rank: Option<f64>,
    pub credit_web: Option<CreditWebInput>,
}

/// Shillbot reputation inputs (`agent_profile.shillbot.derived`).
/// `average_score` is the 0..MAX_SCORE composite from the oracle;
/// `completion_rate` is `total_completed / total_tasks_claimed`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ShillbotInput {
    pub average_score: Option<f64>,
    pub score_max: u64,
    pub completion_rate: Option<f64>,
    /// Raw `total_completed` — used as a confidence gate. An agent
    /// with zero completed tasks contributes nothing to Shillbot
    /// reputation even if `average_score` is somehow set.
    pub total_completed: u64,
}

/// Coordination Game inputs (`agent_profile.coordination_game.derived`).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GameInput {
    pub win_rate: Option<f64>,
    /// Raw `total_games` — confidence gate. Win rate over 1 game
    /// isn't meaningful; we require ≥ 5 games for the signal to
    /// contribute.
    pub total_games: u64,
}

/// Extension-credit web-position inputs (B4). `position` is the agent's
/// recursive-trust-weighted standing in the extension web, already
/// normalized to 0..1 by the web-position indexer.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CreditWebInput {
    pub position: Option<f64>,
    /// Raw extension count — confidence gate. An agent with zero recorded
    /// extensions has no web position; we require ≥ 1 to contribute.
    pub extensions_count: u64,
}

/// Layer 3 curator tier discriminant. Maps to a fixed 0..1 ascription:
/// first-party → 1.0, vetted → 0.7, discovered → 0.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CuratorTier {
    FirstParty,
    Vetted,
    Discovered,
}

impl CuratorTier {
    pub fn ascription(&self) -> f64 {
        match self {
            CuratorTier::FirstParty => 1.0,
            CuratorTier::Vetted => 0.7,
            CuratorTier::Discovered => 0.3,
        }
    }
}

/// Composite trust output: 0..1 score + confidence indicator. The
/// `breakdown` shows per-signal contributions for transparency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    /// Weighted average of the present signals, in 0..1.
    pub score: f64,
    /// Number of signals that contributed (0..=6). Lower numbers
    /// mean less reliable composite.
    pub confidence: u8,
    /// Per-signal breakdown for transparency. Each entry is a
    /// `(name, normalized_value, weight_applied)` triple where
    /// `weight_applied` is the renormalized weight (sums to 1.0
    /// across present signals).
    pub breakdown: Vec<TrustContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustContribution {
    pub signal: String,
    pub value: f64,
    pub weight: f64,
}

/// Compute the composite trust score from the available inputs.
/// Returns `score: 0.0, confidence: 0` when all signals are absent
/// — caller can decide whether to treat that as "unknown" or
/// "untrusted."
///
/// Pure function: no I/O, no panics, deterministic for given inputs.
pub fn compute_trust(inputs: &TrustInputs) -> TrustScore {
    let mut signals: Vec<TrustContribution> = Vec::new();

    // Shillbot — only contributes if `average_score` is present AND
    // `total_completed` ≥ 1 (a single completed task is enough
    // signal; below 5 the confidence is naturally low because the
    // weight-sum is small).
    if let Some(s) = &inputs.shillbot {
        if let Some(avg) = s.average_score {
            if s.total_completed >= 1 && s.score_max > 0 {
                let normalized_score = (avg / s.score_max as f64).clamp(0.0, 1.0);
                // If completion_rate is present, combine it with the
                // average score equally within the Shillbot signal.
                // If it's absent (None), score on quality alone — DO NOT
                // treat missing data as 0% completion. Treating None as
                // 0.0 caps the shillbot signal at 0.5 for legacy agents
                // (e.g. those who completed tasks before the AgentState
                // schema was extended in task #12 and so have
                // total_tasks_claimed=0 with total_completed>0). Surfaced
                // by the 2026-05-04 E2E test session — the founder's
                // wallet showed trust_score=0.4708 instead of the ~0.875
                // its 5 perfect-score completions warranted.
                let combined = match s.completion_rate {
                    Some(rate) => (normalized_score + rate.clamp(0.0, 1.0)) / 2.0,
                    None => normalized_score,
                };
                signals.push(TrustContribution {
                    signal: "shillbot".to_string(),
                    value: combined,
                    weight: WEIGHT_SHILLBOT,
                });
            }
        }
    }

    // EigenTrust — relational graph position (6th signal). Already
    // rank-normalized to 0..1 by the reputation indexer.
    if let Some(e) = inputs.eigentrust {
        signals.push(TrustContribution {
            signal: "eigentrust".to_string(),
            value: e.clamp(0.0, 1.0),
            weight: WEIGHT_EIGENTRUST,
        });
    }

    // Game — requires ≥ 5 games for the signal to be meaningful.
    if let Some(g) = &inputs.game {
        if let Some(wr) = g.win_rate {
            if g.total_games >= 5 {
                signals.push(TrustContribution {
                    signal: "coordination_game".to_string(),
                    value: wr.clamp(0.0, 1.0),
                    weight: WEIGHT_GAME,
                });
            }
        }
    }

    // Curator tier — always contributes when present; categorical.
    if let Some(tier) = &inputs.curator {
        signals.push(TrustContribution {
            signal: "curator".to_string(),
            value: tier.ascription(),
            weight: WEIGHT_CURATOR,
        });
    }

    // AgentRank — already 0..1 normalized by Hyperspace.
    if let Some(rank) = inputs.agent_rank {
        signals.push(TrustContribution {
            signal: "agent_rank".to_string(),
            value: rank.clamp(0.0, 1.0),
            weight: WEIGHT_AGENT_RANK,
        });
    }

    // Credit-web (B4) — extension-credit web-position. Contributes only when
    // the agent has ≥ 1 recorded extension (confidence gate), so a
    // never-extended agent doesn't get a spurious signal.
    if let Some(cw) = &inputs.credit_web {
        if let Some(pos) = cw.position {
            if cw.extensions_count >= 1 {
                signals.push(TrustContribution {
                    signal: "credit_web".to_string(),
                    value: pos.clamp(0.0, 1.0),
                    weight: WEIGHT_CREDIT_WEB,
                });
            }
        }
    }

    if signals.is_empty() {
        return TrustScore {
            score: 0.0,
            confidence: 0,
            breakdown: Vec::new(),
        };
    }

    // Renormalize weights over the present signals so they sum to 1.
    let total_weight: f64 = signals.iter().map(|c| c.weight).sum();
    if total_weight <= 0.0 {
        // Defensive — shouldn't happen given the constants above.
        return TrustScore {
            score: 0.0,
            confidence: 0,
            breakdown: Vec::new(),
        };
    }
    for c in signals.iter_mut() {
        c.weight /= total_weight;
    }

    let score: f64 = signals.iter().map(|c| c.value * c.weight).sum();
    let confidence = signals.len() as u8;

    TrustScore {
        score: score.clamp(0.0, 1.0),
        confidence,
        breakdown: signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn empty_inputs_yields_zero_score_and_zero_confidence() {
        let result = compute_trust(&TrustInputs::default());
        assert_eq!(result.score, 0.0);
        assert_eq!(result.confidence, 0);
        assert!(result.breakdown.is_empty());
    }

    #[test]
    fn credit_web_contributes_only_above_the_gate() {
        // extensions_count = 0 → gated out (no contribution).
        let gated = compute_trust(&TrustInputs {
            eigentrust: None,
            credit_web: Some(CreditWebInput {
                position: Some(0.8),
                extensions_count: 0,
            }),
            ..Default::default()
        });
        assert_eq!(
            gated.confidence, 0,
            "credit_web gated out below 1 extension"
        );

        // extensions_count ≥ 1 → contributes; single signal renormalizes to 1.
        let active = compute_trust(&TrustInputs {
            eigentrust: None,
            credit_web: Some(CreditWebInput {
                position: Some(0.8),
                extensions_count: 3,
            }),
            ..Default::default()
        });
        assert_eq!(active.confidence, 1);
        assert!(approx_eq(active.score, 0.8));
        assert_eq!(active.breakdown[0].signal, "credit_web");
        assert!(approx_eq(active.breakdown[0].weight, 1.0));
    }

    #[test]
    fn credit_web_renormalizes_alongside_shillbot() {
        // shillbot (0.45) + credit_web (0.20) → weights renormalize over 0.65.
        let result = compute_trust(&TrustInputs {
            eigentrust: None,
            shillbot: Some(ShillbotInput {
                average_score: Some(1_000_000.0),
                score_max: 1_000_000,
                completion_rate: Some(1.0),
                total_completed: 5,
            }),
            credit_web: Some(CreditWebInput {
                position: Some(0.5),
                extensions_count: 2,
            }),
            ..Default::default()
        });
        assert_eq!(result.confidence, 2);
        let total = 0.45 + 0.20;
        let sb = result
            .breakdown
            .iter()
            .find(|c| c.signal == "shillbot")
            .unwrap();
        let cw = result
            .breakdown
            .iter()
            .find(|c| c.signal == "credit_web")
            .unwrap();
        assert!(approx_eq(sb.weight, 0.45 / total));
        assert!(approx_eq(cw.weight, 0.20 / total));
    }

    #[test]
    fn single_signal_renormalizes_weight_to_one() {
        // Only curator present.
        let result = compute_trust(&TrustInputs {
            eigentrust: None,
            curator: Some(CuratorTier::FirstParty),
            ..Default::default()
        });
        assert_eq!(result.confidence, 1);
        assert_eq!(result.breakdown.len(), 1);
        // Weight should be 1.0 after renormalization (was 0.15
        // pre-renorm against absent siblings).
        assert!(approx_eq(result.breakdown[0].weight, 1.0));
        // First-party ascription = 1.0, weight = 1.0 → score = 1.0.
        assert!(approx_eq(result.score, 1.0));
    }

    #[test]
    fn shillbot_below_minimum_completed_is_skipped() {
        // average_score is set but total_completed = 0 → signal skipped.
        let result = compute_trust(&TrustInputs {
            eigentrust: None,
            shillbot: Some(ShillbotInput {
                average_score: Some(800_000.0),
                score_max: 1_000_000,
                completion_rate: Some(0.9),
                total_completed: 0,
            }),
            ..Default::default()
        });
        assert_eq!(result.confidence, 0);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn shillbot_combines_average_score_and_completion_rate_equally() {
        // average = 800k / 1M = 0.8; completion = 1.0; combined = 0.9
        let result = compute_trust(&TrustInputs {
            eigentrust: None,
            shillbot: Some(ShillbotInput {
                average_score: Some(800_000.0),
                score_max: 1_000_000,
                completion_rate: Some(1.0),
                total_completed: 10,
            }),
            ..Default::default()
        });
        assert_eq!(result.confidence, 1);
        let s = &result.breakdown[0];
        assert_eq!(s.signal, "shillbot");
        assert!(approx_eq(s.value, 0.9));
        // Single signal → renormalized weight = 1.0.
        assert!(approx_eq(s.weight, 1.0));
        assert!(approx_eq(result.score, 0.9));
    }

    #[test]
    fn shillbot_signal_uses_score_alone_when_completion_rate_is_none() {
        // Legacy-agent case: agents who completed tasks before the
        // AgentState schema extension in task #12 have
        // total_tasks_claimed=0 with total_completed>0, so the
        // orchestrator/MCP returns completion_rate=None. Treating None
        // as 0.0 (the prior behavior) capped their shillbot signal at
        // 0.5*avg, halving the score they'd earned. The fix: when
        // completion_rate is None, score on quality alone.
        //
        // average = 1.0 (perfect score), completion = None →
        // combined value should be 1.0, not 0.5.
        let result = compute_trust(&TrustInputs {
            eigentrust: None,
            shillbot: Some(ShillbotInput {
                average_score: Some(1_000_000.0),
                score_max: 1_000_000,
                completion_rate: None,
                total_completed: 5,
            }),
            ..Default::default()
        });
        assert_eq!(result.confidence, 1);
        let s = &result.breakdown[0];
        assert_eq!(s.signal, "shillbot");
        assert!(
            approx_eq(s.value, 1.0),
            "legacy agent with perfect score should score 1.0 on shillbot signal, got {}",
            s.value
        );
        assert!(approx_eq(result.score, 1.0));
    }

    #[test]
    fn game_below_minimum_games_is_skipped() {
        // win_rate set but total_games = 4 (below the 5-game floor) → skipped.
        let result = compute_trust(&TrustInputs {
            eigentrust: None,
            game: Some(GameInput {
                win_rate: Some(0.75),
                total_games: 4,
            }),
            ..Default::default()
        });
        assert_eq!(result.confidence, 0);
    }

    #[test]
    fn curator_ascription_table() {
        assert_eq!(CuratorTier::FirstParty.ascription(), 1.0);
        assert_eq!(CuratorTier::Vetted.ascription(), 0.7);
        assert_eq!(CuratorTier::Discovered.ascription(), 0.3);
    }

    #[test]
    fn all_four_signals_yield_canonical_weighted_average() {
        // Construct inputs where every signal contributes 1.0 → output
        // should be exactly 1.0 regardless of the weight constants.
        let result = compute_trust(&TrustInputs {
            eigentrust: None,
            shillbot: Some(ShillbotInput {
                average_score: Some(1_000_000.0),
                score_max: 1_000_000,
                completion_rate: Some(1.0),
                total_completed: 50,
            }),
            game: Some(GameInput {
                win_rate: Some(1.0),
                total_games: 100,
            }),
            curator: Some(CuratorTier::FirstParty),
            agent_rank: Some(1.0),
            credit_web: Some(CreditWebInput {
                position: Some(1.0),
                extensions_count: 5,
            }),
        });
        assert_eq!(result.confidence, 5);
        assert!(approx_eq(result.score, 1.0));
        // Weights sum to 1.0 across all five signals.
        let weight_sum: f64 = result.breakdown.iter().map(|c| c.weight).sum();
        assert!(approx_eq(weight_sum, 1.0));
    }

    #[test]
    fn weights_sum_to_one_after_renormalization_with_two_signals() {
        // Only shillbot + game contribute; curator + agent_rank absent.
        // Pre-renorm weights: 0.45 + 0.15 = 0.6. After renorm: each
        // scales by 1/0.6.
        let result = compute_trust(&TrustInputs {
            eigentrust: None,
            shillbot: Some(ShillbotInput {
                average_score: Some(500_000.0),
                score_max: 1_000_000,
                completion_rate: Some(0.8),
                total_completed: 20,
            }),
            game: Some(GameInput {
                win_rate: Some(0.6),
                total_games: 30,
            }),
            ..Default::default()
        });
        assert_eq!(result.confidence, 2);
        let weight_sum: f64 = result.breakdown.iter().map(|c| c.weight).sum();
        assert!(approx_eq(weight_sum, 1.0));
        // Shillbot's weight after renorm: 0.45 / 0.6 = 0.75.
        let shillbot_w = result
            .breakdown
            .iter()
            .find(|c| c.signal == "shillbot")
            .unwrap()
            .weight;
        assert!(approx_eq(shillbot_w, 0.75));
    }

    #[test]
    fn out_of_range_inputs_are_clamped() {
        // score_max smaller than average_score → ratio > 1; clamp to 1.
        // win_rate > 1 → clamp to 1. agent_rank > 1 → clamp to 1.
        // completion_rate > 1 → clamp to 1.
        let result = compute_trust(&TrustInputs {
            eigentrust: None,
            shillbot: Some(ShillbotInput {
                average_score: Some(2_000_000.0),
                score_max: 1_000_000,
                completion_rate: Some(1.5),
                total_completed: 10,
            }),
            game: Some(GameInput {
                win_rate: Some(2.0),
                total_games: 100,
            }),
            curator: Some(CuratorTier::FirstParty),
            agent_rank: Some(5.0),
            credit_web: Some(CreditWebInput {
                position: Some(5.0),
                extensions_count: 1,
            }),
        });
        assert!(result.score <= 1.0);
        for c in &result.breakdown {
            assert!(
                c.value <= 1.0,
                "signal {} value {} not clamped",
                c.signal,
                c.value
            );
        }
    }
}
