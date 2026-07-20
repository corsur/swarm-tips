# Wide but Shallow

*The generative structure of technical-interview problems, and the myth of company-specific preparation.*

A paper + reproducible pipeline. Headline findings (see `paper.tex`): (1) big-tech firms are
largely **interchangeable** in algorithmic interview profile — scheme-level bias-corrected Cramér's
**V = 0.091** (95% bootstrap CI [0.06, 0.12], small at most); six of the seven are mutually
indistinguishable and only **Uber** stands out (leaning on graph relaxation); (2) on a
**pre-registered random sample** of 100 problems (seed fixed before the draw), **71 are one of
four recursion schemes** — streaming fold, DP, graph relaxation, bisection — each backed by a proof:
69 machine-checked here in Lean, 2 citing existing formal proofs of MST (LC 1489) / graph-bridges
(LC 1192); 95% Wilson [0.61, 0.79]. The other 29 are a genuine tail (incl. Add Digits, whose optimal solution is an O(1) formula). Every classification was
cross-checked against the problem's published editorial (`EDITORIAL_VERIFICATION.md`).
So **~three-quarters of interview problems reduce to four ideas**, the streaming fold the
single most common, unifying techniques memorized as unrelated (two-pointers, hashing, stack,
prefix-sum, XOR, reversal).

## The paper

- `paper.tex` — single source, arXiv-safe preamble.
- `./build.sh all` → `paper.pdf` (tectonic), `paper.html` + `paper.md` (pandoc, for blog).
  - `./build.sh arxiv` — PDF only (upload `paper.tex` + `figs/*.png`; arXiv category **cs.CY**).
  - `./build.sh blog` — HTML/Markdown for Substack / static site.
  - SIGCSE/ITiCSE short paper: trim the formal sections; the empirical half is the contribution.

## The pipeline

```
data/raw/{company}__{window}.json   rank-ordered frequency lists (35 strata, 7 companies)
data/raw/problems.json              num -> {name, diff}  (769 problems)
        │
        ├── taxonomy.json           43 families -> 20 macros -> core/periphery
        ├── do_labels.py            -> labels.csv   (num -> family; validated complete)
        └── analyze.py              -> stats + figs + frequencies.csv
```

- **`do_labels.py`** — the labeling (simplest interviewer-accepted solution rule). Validates
  every problem is covered with a valid family. (`make_labels.py` emits a blank template.)
- **`analyze.py`** — permutation test + Cramér's V, BH-corrected pairwise, core/periphery,
  family concentration, PCA. `python analyze.py --window six-months --weight count`.
  `--weight rank` is the robustness pass.
- **`lproofs/`** — Lean 4 / Mathlib project (build: `cd lproofs && lake build`; all standard
  axioms, no `sorry`).
  - **`lake exe gate`** — the mechanical genuineness gate (added 2026-07-19, replacing the former
    hand-maintained `NOT_GENUINE` blacklist in `build_manifest.py`). A Lean metaprogram recomputes
    every certificate's verdict from the **elaborated environment**: the file designates `sol`;
    the *types* of `cls` and `corr` reference it; a closed kernel-checked **ground-instance
    theorem** (`vec*`) evaluates `sol` on a concrete input (usually the problem's published
    example — an abstract placeholder cannot produce one); and the transitive axiom closure of
    all three stays within `{propext, Quot.sound, Classical.choice}` (mechanically rejecting
    `sorry` and `native_decide`). Emits `gate.csv`, consumed by `build_manifest.py`; all 69
    in-sample certs pass, and every certificate the 2026-06-19 panel had blacklisted fails the
    gate naturally, with no list consulted.
  - Deferred (recorded, not planned): a mutation harness (corr must fail on a gutted `sol`) is
    largely subsumed by the gate's sol-reference check — definitional mutants already break the
    `rfl`/`unfold`-tied proofs; its only residual value is against self-referentially trivial
    `corr` statements (e.g. `sol xs = sol xs`), which no current certificate has.
  - `Generators.lean` — `range_fold` (group scan), `agg_hom` (semiring polymorphism), `bellman_*` (relaxation = least fixpoint).
  - `Classify.lean` — per-problem classification certs (Climbing Stairs = DP fold, Range Sum = scan, reachability = lfp).
  - `Patterns.lean` — the **four recursion schemes** (~80% of load): `IsFold` with 5 instances proving prefix-sum/XOR/reverse/running-max/seen-set are *one fold* (the merge); `bisection_threshold`/`bisection_isLeast` (binary search); `depth_isCata` (tree), `subsets_card` (backtracking).

## Headline numbers (data snapshot June 2026)

- Between-firm divergence (scheme level): **bias-corrected V = 0.091** (95% bootstrap CI
  [0.06, 0.12], small at most) — the paper's headline (`significance.py`); the omnibus is
  significant (6-month permutation **p = 0.0003**) yet the effect size is bounded below moderate.
  The finer 20-family grouping is smaller still (V = 0.064, `--level family`).
- Google/Amazon/Microsoft/Meta/Bloomberg/Apple: **mutually indistinguishable**. Uber is the lone
  outlier (largest gap, vs Google, is a "small" effect).
- Family concentration: **Gini 0.30**; 6 families = 50%, but **4 recursion schemes = ~82.3%** (fold 48.4, DP 18.0, relaxation 8.2, bisection 7.7; cross-check vs 4 empirical generators = 67.4%).
- Algebraic fraction of load: **~30%** (semiring catamorphism + lattice fixpoint + monoid scan; ~20% strict) — a subset of the ~82% scheme-classifiable.
- Label robustness: company-comparison headlines hold in **100%** of 2,000 Monte-Carlo relabelings; frequency-weighted four-scheme *load* coverage (a distinct measure from the 71/100 per-problem proof count) stays **79–84%** (`sensitivity.py`).

## Status / before submission

Done: data, labeling, analysis, Lean core (**proof-classified coverage on the pre-registered sample
(seed 20260619, fixed before drawing): 71/100 proven to be one of the four ideas — 69 machine-checked
in Lean + 2 citation-backed (1489 MST → AFP Kruskal, 1192 bridges → AFP DFS_Framework); all 71 in-scheme
classified, none refuted; the 29 tail genuinely outside the four (incl. 258 Add Digits, reclassified —
optimal is O(1) arithmetic); old seed-7 76/100 was the best-of-2000
draw and is discarded** — see PREREGISTRATION.md, CITATIONS.md, EDITORIAL_VERIFICATION.md; cls≡corr closed
for the flagship folds Kadane/Product/Single Number), scheme-level
residual figure, **label sensitivity** (`sensitivity.py`: every headline holds in 100% of 2,000
relabelings), Related-work section + citations, draft. **Outstanding:** true inter-rater κ (a second
human rater; the 2,000-draw sensitivity Monte-Carlo is the current robustness substitute). Certificate
strength varies — full correctness for the flagship folds, a one-directional property otherwise.

## Data / ToS

Raw per-problem frequency tables are **withheld** (platform terms); only derived aggregates
are published. `analyze.py` reproduces every number from the local data.
