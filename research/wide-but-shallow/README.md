# Wide but Shallow

*Technical-interview problems are four ideas in many costumes — and company-specific preparation is mostly a myth.*

This is the public artifact for the paper **"Wide but Shallow"** (`paper.pdf`): the derived data, the
statistical code, and the complete Lean 4 / Mathlib proof development behind its claims.

## The finding, in one paragraph

The roughly twenty surface "patterns" that interview-prep material tells you to memorize are mostly **four
recursion schemes** — streaming fold, recursive decomposition (dynamic programming), graph relaxation, and
bisection — with the streaming fold the single most common. On a uniform random sample of 100 distinct
problems, **71 carry a machine-checked Lean certificate** that the problem is genuinely solvable by its
assigned scheme and that a transcribed solution is correct (95% Wilson interval [0.61, 0.79]; no `sorry`;
standard axioms). Separately, the seven firms studied ask nearly the same *mix* of schemes
(scheme-level bias-corrected Cramér's *V* = 0.091, 95% bootstrap CI [0.06, 0.12] — a small effect at most,
bounded well below a moderate one); six of the seven are mutually indistinguishable and only Uber stands out.
We measure the structure of *commonly-asked* problems, not interview difficulty or the payoff of any prep
regimen.

## What's here

```
paper.pdf, paper.tex          the paper
data/sample.csv               the uniform random sample of 100 problems; the `proven` column
                              re-derives the headline 71/100
data/certs.csv                per-problem certification manifest, derived from the proofs by
                              scripts/build_manifest.py (proven = cls AND corr AND no sorry)
data/family_scheme_rule.csv   the fixed family→scheme rule (set in advance; families absent → tail)
data/labels.csv               surface family per problem (num → family)
data/frequencies.csv          tidy long table of the derived per-firm family frequencies
                              (= build_long('count'); the company effect size reproduces from this)
data/taxonomy.json            family → macro-family → class map
scripts/                      sampling, classification counting, and the company effect-size analysis
proofs/                       the Lean 4 / Mathlib development (run `lake exe cache get && lake build`)
```

## Reproducing the numbers

- **Coverage (71/100).** The count is the `proven=True` rows of `data/sample.csv`. To re-derive it from the
  proofs themselves, build the Lean development (below), then `python scripts/build_manifest.py` regenerates
  `data/certs.csv` (a problem is certified iff its file has `theorem cls` **and** `theorem corr` and no
  `sorry`) and `python scripts/sample.py` rewrites the `proven` column of `data/sample.csv` from that
  manifest.
- **Company similarity (*V* = 0.091).** `python scripts/significance.py` recomputes the scheme-level
  bias-corrected effect size, its (bias-corrected) bootstrap CI, the equivalence verdict, and the per-firm
  pairwise comparisons (`--level family` gives the finer 20-family check, *V* = 0.064). It reproduces from
  `data/frequencies.csv` — **no raw scrape required**.
- **Proofs.** In `proofs/`, `lake exe cache get && lake build` checks every certificate; `#print axioms` on
  the top-level theorems confirms only Lean's standard axioms are used. `lake-manifest.json` and
  `lean-toolchain` pin the Mathlib revision and compiler so the build is reproducible.

## Data note (please read)

The **raw per-company problem-frequency tables are deliberately withheld** per the source platform's terms
and are *not* included here. Only *derived* statistics are released: the sample, the rule, the per-problem
family labels, the certification manifest, and the tidy `frequencies.csv`. The two headline numbers above
reproduce from these derived files plus the proof development. Two scripts are included for transparency but
need the withheld raw scrape, so they are **not runnable from this public artifact**: `sensitivity.py` (the
2,000-draw label-robustness Monte-Carlo) and `company_analysis.py` (the per-firm scheme-level audit).

## Authorship

Single human author, with substantial AI assistance for drafting, statistical code, and the Lean
formalization; all claims, numbers, and proofs were checked by the author. The machine-checked results are,
by construction, independently verifiable by re-running the proof development. See the paper's
"Reproducibility and AI-use disclosure" section.
