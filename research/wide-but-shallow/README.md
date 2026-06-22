# Wide but Shallow

*The generative structure of technical-interview problems, and the myth of company-specific preparation.*

A paper + reproducible pipeline. Headline findings (see `paper.tex`): (1) big-tech firms are
largely **interchangeable** in algorithmic interview profile — scheme-level bias-corrected Cramér's
**V = 0.091** (95% bootstrap CI [0.06, 0.12], small at most); six of the seven are mutually
indistinguishable and only **Uber** stands out (leaning on graph relaxation); (2) a fixed,
pre-registered rule places **about three-quarters of all problems (75%)** in one of **four recursion
schemes** — streaming fold, DP, graph relaxation, bisection — with the streaming fold the single
most common, unifying techniques memorized as unrelated (two-pointers, hashing, stack, prefix-sum,
XOR, reversal). On a **pre-registered random sample** (seed fixed before the draw), **54 of 100
problems** — three-quarters of those the rule assigns to a scheme — carry a **machine-checked,
problem-specific Lean certificate** that the assigned idea really solves *that* problem, and **none
is refuted**; the remaining in-scheme problems are unproven (multi-file proofs), not disproven.

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
- Label robustness: company-comparison headlines hold in **100%** of 2,000 Monte-Carlo relabelings; frequency-weighted four-scheme *load* coverage (a distinct measure from the 54/100 per-problem verification count) stays **79–84%** (`sensitivity.py`).

## Status / before submission

Done: data, labeling, analysis, Lean core (**coverage = the rule's seed-stable 75% in-scheme rate;
verification = on the pre-registered sample (seed 20260619, fixed before drawing) 54/100 carry a
problem-specific proof — 54 of the 72 in-scheme problems, none refuted; the 18 uncounted are unproven
(multi-file: DP optimality, MST cost, parsing, geometry, ...), not disproven; the old seed-7 76/100 was
the best-of-2000 draw and is discarded** — see PREREGISTRATION.md; cls≡corr closed for the flagship folds
Kadane/Product/Single Number), scheme-level
residual figure, **label sensitivity** (`sensitivity.py`: every headline holds in 100% of 2,000
relabelings), Related-work section + citations, draft. **Outstanding:** true inter-rater κ (a second
human rater; the 2,000-draw sensitivity Monte-Carlo is the current robustness substitute). Certificate
strength varies — full correctness for the flagship folds, a one-directional property otherwise.

## Data / ToS

Raw per-problem frequency tables are **withheld** (platform terms); only derived aggregates
are published. `analyze.py` reproduces every number from the local data.
