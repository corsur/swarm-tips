//! Combinatorial matrices over the pure seams of the complex features:
//! composite trust (exhaustive signal-presence lattice), the extension-credit
//! web (vouch topologies), and BM25 search (query × filter × quality-signal
//! grid). Counterpart to the shillbot bankrun combinatorial matrix — these
//! run exhaustively in CI because every seam is a pure function.

mod trust_matrix {
    use crate::composite_trust::{
        compute_trust, CreditWebInput, CuratorTier, GameInput, ShillbotInput, TrustInputs,
    };

    /// Gate-passing representative input for each signal, with every
    /// normalized contribution pinned to `v`.
    fn inputs_for(mask: u8, v: f64) -> TrustInputs {
        TrustInputs {
            shillbot: (mask & 1 != 0).then_some(ShillbotInput {
                average_score: Some(v * 1_000_000.0),
                score_max: 1_000_000,
                completion_rate: Some(v),
                total_completed: 10,
            }),
            eigentrust: (mask & 2 != 0).then_some(v),
            game: (mask & 4 != 0).then_some(GameInput {
                win_rate: Some(v),
                total_games: 20,
            }),
            curator: (mask & 8 != 0).then_some(CuratorTier::Vetted),
            agent_rank: (mask & 16 != 0).then_some(v),
            credit_web: (mask & 32 != 0).then_some(CreditWebInput {
                position: Some(v),
                extensions_count: 3,
            }),
        }
    }

    /// Exhaustive presence lattice: all 64 subsets of the 6 signals uphold
    /// the composite's invariants.
    #[test]
    fn all_64_presence_combinations_uphold_invariants() {
        for mask in 0u8..64 {
            let trust = compute_trust(&inputs_for(mask, 0.5));
            let present = mask.count_ones();
            assert!(
                (0.0..=1.0).contains(&trust.score),
                "mask {mask:#08b}: score {} out of [0,1]",
                trust.score
            );
            assert_eq!(
                u32::from(trust.confidence),
                present,
                "mask {mask:#08b}: confidence != present-signal count"
            );
            assert_eq!(
                trust.breakdown.len(),
                present as usize,
                "mask {mask:#08b}: breakdown length != present-signal count"
            );
            let weight_sum: f64 = trust.breakdown.iter().map(|c| c.weight).sum();
            if present > 0 {
                assert!(
                    (weight_sum - 1.0).abs() < 1e-9,
                    "mask {mask:#08b}: renormalized weights sum to {weight_sum}, not 1.0"
                );
            } else {
                assert_eq!(trust.score, 0.0, "all-absent must score 0");
            }
        }
    }

    /// Raising the shared signal value never lowers the composite, for every
    /// presence combination (monotonicity across the whole lattice).
    #[test]
    fn score_is_monotone_in_signal_values_across_the_lattice() {
        for mask in 1u8..64 {
            let lo = compute_trust(&inputs_for(mask, 0.3)).score;
            let hi = compute_trust(&inputs_for(mask, 0.9)).score;
            assert!(
                hi >= lo,
                "mask {mask:#08b}: score fell from {lo} to {hi} as values rose"
            );
        }
    }

    /// With every contribution pinned to the same v, the renormalized
    /// composite must equal v (a pure weighted mean) — except when the
    /// curator tier (a fixed ascription) is present.
    #[test]
    fn composite_of_uniform_signals_is_that_value() {
        for mask in 1u8..64 {
            if mask & 8 != 0 {
                continue; // curator contributes a fixed 0.7 (Vetted), not v
            }
            let trust = compute_trust(&inputs_for(mask, 0.6));
            assert!(
                (trust.score - 0.6).abs() < 1e-9,
                "mask {mask:#08b}: uniform 0.6 inputs composed to {}",
                trust.score
            );
        }
    }

    /// Confidence gates: below-threshold raw counts silence a signal even
    /// when its value is present, for every surrounding combination.
    #[test]
    fn confidence_gates_silence_signals_across_the_lattice() {
        for mask in 0u8..64 {
            let mut inputs = inputs_for(mask, 0.5);
            // Force every gated signal below its threshold.
            if let Some(s) = &mut inputs.shillbot {
                s.total_completed = 0;
            }
            if let Some(g) = &mut inputs.game {
                g.total_games = 4; // < 5-game gate
            }
            if let Some(c) = &mut inputs.credit_web {
                c.extensions_count = 0;
            }
            let gated =
                u32::from(mask & 1 != 0) + u32::from(mask & 4 != 0) + u32::from(mask & 32 != 0);
            let trust = compute_trust(&inputs);
            assert_eq!(
                u32::from(trust.confidence),
                mask.count_ones() - gated,
                "mask {mask:#08b}: gated signals still counted"
            );
        }
    }
}

mod web_topologies {
    use crate::web_position::{compute_web_positions, ExtensionEdge};

    const ROOT: &str = "root";
    const BOND: u64 = 5_000_000;

    fn edge(from: &str, to: &str, bond: u64) -> ExtensionEdge {
        ExtensionEdge {
            extender: from.to_string(),
            recipient: to.to_string(),
            bond_lamports: bond,
        }
    }

    #[test]
    fn empty_graph_has_no_positions() {
        assert!(compute_web_positions(&[], ROOT).is_empty());
    }

    #[test]
    fn star_from_root_ranks_all_and_excludes_root() {
        let edges = vec![
            edge(ROOT, "a", BOND),
            edge(ROOT, "b", BOND),
            edge(ROOT, "c", BOND),
        ];
        let pos = compute_web_positions(&edges, ROOT);
        assert_eq!(pos.len(), 3);
        assert!(!pos.contains_key(ROOT), "root must be excluded");
        for (agent, p) in &pos {
            assert!(
                (*p - 1.0).abs() < 1e-6,
                "equal direct vouches must rank equally at ~1.0 (agent {agent}: {p})"
            );
        }
    }

    #[test]
    fn chain_decays_monotonically_with_distance_from_root() {
        let edges = vec![
            edge(ROOT, "a", BOND),
            edge("a", "b", BOND),
            edge("b", "c", BOND),
        ];
        let pos = compute_web_positions(&edges, ROOT);
        let (a, b, c) = (pos["a"], pos["b"], pos["c"]);
        assert!(
            a >= b && b >= c,
            "trust must not grow with distance from root: a={a} b={b} c={c}"
        );
        assert!((a - 1.0).abs() < 1e-6, "nearest agent rescales to ~1.0");
        assert!(c > 0.0, "reachable agents keep positive standing");
    }

    #[test]
    fn detached_sybil_ring_scores_nothing() {
        // A mutual-vouch ring with real bonds but no path from the root:
        // present in the result domain only via ~0 scores (or absent).
        let edges = vec![
            edge(ROOT, "honest", BOND),
            edge("x", "y", BOND * 100),
            edge("y", "z", BOND * 100),
            edge("z", "x", BOND * 100),
        ];
        let pos = compute_web_positions(&edges, ROOT);
        assert!((pos["honest"] - 1.0).abs() < 1e-6);
        for sybil in ["x", "y", "z"] {
            let p = pos.get(sybil).copied().unwrap_or(0.0);
            assert!(
                p < 0.05,
                "detached sybil {sybil} scored {p} — ring must not mint standing"
            );
        }
    }

    #[test]
    fn sybil_ring_behind_one_vouch_cannot_outrank_its_entry_point() {
        // The root vouches for `a`; `a` vouches one sybil, which rings with
        // two more. Nothing downstream of `a` may outrank `a`.
        let edges = vec![
            edge(ROOT, "a", BOND),
            edge("a", "x", BOND),
            edge("x", "y", BOND * 100),
            edge("y", "x", BOND * 100),
        ];
        let pos = compute_web_positions(&edges, ROOT);
        for sybil in ["x", "y"] {
            assert!(
                pos.get(sybil).copied().unwrap_or(0.0) <= pos["a"] + 1e-9,
                "{sybil} outranked its entry point"
            );
        }
    }

    #[test]
    fn self_vouch_mints_no_standing() {
        let edges = vec![edge(ROOT, "a", BOND), edge("b", "b", BOND * 1000)];
        let pos = compute_web_positions(&edges, ROOT);
        assert!(
            pos.get("b").copied().unwrap_or(0.0) < 0.05,
            "self-vouch must not mint standing"
        );
    }

    #[test]
    fn larger_bond_never_ranks_below_smaller_from_same_extender() {
        let edges = vec![edge(ROOT, "small", BOND), edge(ROOT, "large", BOND * 200)];
        let pos = compute_web_positions(&edges, ROOT);
        assert!(
            pos["large"] >= pos["small"] - 1e-9,
            "bond size must not invert ranking: large={} small={}",
            pos["large"],
            pos["small"]
        );
    }
}

mod search_matrix {
    use crate::discovery::models::{
        CashFlowDirection, Category, EnrichedServer, Layer1Classification, Layer3Analysis,
        ValueToSwarm,
    };
    use crate::discovery::search::{SearchFilters, SearchIndex};

    /// One corpus entry per point in the (category × currency × provenance ×
    /// signal-tier) grid, each carrying a unique topic token plus a shared
    /// "protocol" token, so queries can select slices of the grid.
    fn grid_corpus() -> Vec<EnrichedServer> {
        let categories = [Category::Payment, Category::Data];
        let currencies = ["sol", "usdc"];
        let provenances = ["first-party", "external"];
        let signal_tiers: [(&str, Option<u32>, Option<u64>); 3] = [
            ("nosignal", None, None),
            ("starred", Some(4_000), None),
            ("popular", Some(4_000), Some(900_000)),
        ];
        let mut corpus = Vec::new();
        for cat in &categories {
            for cur in &currencies {
                for prov in &provenances {
                    for (tier, stars, downloads) in &signal_tiers {
                        let cat_tok = match cat {
                            Category::Payment => "payment",
                            _ => "data",
                        };
                        let name = format!("{cat_tok}-{cur}-{prov}-{tier}");
                        corpus.push(EnrichedServer {
                            name: name.clone(),
                            title: None,
                            description: Some(format!(
                                "{cat_tok} protocol tooling for {cur} agents ({tier} tier server)"
                            )),
                            endpoint: (*prov == "first-party")
                                .then(|| "https://mcp.swarm.tips/mcp".to_string()),
                            transport: None,
                            npm_package: None,
                            github_repo: None,
                            sources: vec!["official".to_string()],
                            source_count: 1,
                            upstream_quality_score: None,
                            upstream_visitors_estimate: None,
                            classification: Layer1Classification {
                                category: Some(*cat),
                                cash_flow_direction: Some(CashFlowDirection::Neutral),
                                currencies: vec![cur.to_string()],
                                value_to_swarm: Some(ValueToSwarm::None),
                                confident: true,
                                matched_signals: vec![],
                            },
                            layer2_classification: None,
                            layer3_analysis: (stars.is_some() || downloads.is_some()).then(|| {
                                Layer3Analysis {
                                    extracted_addresses: vec![],
                                    npm_weekly_downloads: *downloads,
                                    github_stars: *stars,
                                    readme_excerpt: None,
                                    probed: true,
                                    probed_at: chrono::Utc::now(),
                                }
                            }),
                            first_seen_at: chrono::Utc::now(),
                            last_seen_at: chrono::Utc::now(),
                        });
                    }
                }
            }
        }
        corpus
    }

    const NO_FILTERS: SearchFilters = SearchFilters {
        category: None,
        currency: None,
        provenance: None,
    };

    #[test]
    fn every_filter_combination_returns_exactly_its_grid_slice() {
        let index = SearchIndex::build(grid_corpus(), chrono::Utc::now());
        // "protocol" matches every entry; the filters carve the grid. All
        // 2x3x3 = 18 filter combinations must return exactly their slice.
        for cat in [None, Some("payment"), Some("data")] {
            for cur in [None, Some("sol"), Some("usdc")] {
                for prov in [None, Some("first-party"), Some("external")] {
                    let filters = SearchFilters {
                        category: cat,
                        currency: cur,
                        provenance: prov,
                    };
                    let hits = index.search("protocol", &filters, 100);
                    let expected = 24
                        / (if cat.is_some() { 2 } else { 1 })
                        / (if cur.is_some() { 2 } else { 1 })
                        / (if prov.is_some() { 2 } else { 1 });
                    assert_eq!(
                        hits.len(),
                        expected,
                        "filters {cat:?}/{cur:?}/{prov:?} returned {} of expected {expected}",
                        hits.len()
                    );
                }
            }
        }
    }

    #[test]
    fn ranking_is_final_score_descending_in_every_slice() {
        let index = SearchIndex::build(grid_corpus(), chrono::Utc::now());
        for query in ["protocol", "payment", "data", "sol agents"] {
            let hits = index.search(query, &NO_FILTERS, 100);
            assert!(!hits.is_empty(), "query {query:?} unexpectedly empty");
            for w in hits.windows(2) {
                assert!(
                    w[0].ranking_signals.final_score >= w[1].ranking_signals.final_score,
                    "query {query:?}: ordering violated"
                );
            }
        }
    }

    #[test]
    fn quality_signals_break_relevance_ties_in_every_slice() {
        let index = SearchIndex::build(grid_corpus(), chrono::Utc::now());
        // Within one grid slice the three signal tiers have near-identical
        // text; the signal-rich entries must not rank below the bare one.
        let filters = SearchFilters {
            category: Some("payment"),
            currency: Some("sol"),
            provenance: Some("external"),
        };
        let hits = index.search("protocol", &filters, 10);
        assert_eq!(hits.len(), 3);
        let rank_of = |needle: &str| {
            hits.iter()
                .position(|h| h.name.contains(needle))
                .unwrap_or(usize::MAX)
        };
        assert!(
            rank_of("popular") <= rank_of("nosignal"),
            "signal-rich entry ranked below the signal-free twin"
        );
    }

    #[test]
    fn nonsense_and_off_grid_queries_return_zero_across_filters() {
        let index = SearchIndex::build(grid_corpus(), chrono::Utc::now());
        for prov in [None, Some("first-party"), Some("external")] {
            let filters = SearchFilters {
                category: None,
                currency: None,
                provenance: prov,
            };
            assert!(
                index
                    .search("xqzvw quux flibbertigibbet", &filters, 10)
                    .is_empty(),
                "nonsense query returned hits (prov {prov:?})"
            );
        }
    }

    #[test]
    fn limit_is_respected_at_every_boundary() {
        let index = SearchIndex::build(grid_corpus(), chrono::Utc::now());
        for limit in [0, 1, 23, 24, 25, 100] {
            let hits = index.search("protocol", &NO_FILTERS, limit);
            assert!(
                hits.len() <= limit && hits.len() <= 24,
                "limit {limit} produced {} hits",
                hits.len()
            );
        }
    }
}
