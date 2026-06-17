# Wide but Shallow

*Technical-interview problems are four ideas in many costumes — and company-specific preparation is mostly a myth.*

This is the public artifact for the paper **"Wide but Shallow"** (`paper.pdf`): the derived data, the
statistical code, and the complete Lean 4 / Mathlib proof development behind its claims.

## The finding, in one paragraph

The roughly twenty surface "patterns" that interview-prep material tells you to memorize are mostly **four
recursion schemes** — streaming fold, recursive decomposition (dynamic programming), graph relaxation, and
bisection — with the streaming fold the single most common. On a uniform random sample of 100 distinct
problems, **63 carry a machine-checked Lean certificate** that the problem is genuinely solvable by its
assigned scheme and that a transcribed solution is correct (95% Wilson interval [0.53, 0.72]; no `sorry`;
standard axioms). Separately, the seven firms studied ask nearly the same *mix* of schemes (scheme-level
bias-corrected Cramér's *V* ≈ 0.09, 95% bootstrap CI [0.08, 0.25] — small, and bounded below a moderate
effect). We measure the structure of *commonly-asked* problems, not interview difficulty or the payoff of
any prep regimen.

## What's here

```
paper.pdf, paper.tex          the paper
data/sample.csv               the uniform random sample of 100 problems; the `proven` column
                              re-derives the headline 63/100
data/family_scheme_rule.csv   the fixed family→scheme rule (set in advance; families absent → tail)
data/labels.csv               surface family per problem (num → family)
data/certs.csv                per-problem certification manifest (derived from the proofs)
scripts/                      sampling, classification counting, and the company effect-size analysis
proofs/                       the Lean 4 / Mathlib development (run `lake exe cache get && lake build`)
```

## Reproducing the numbers

- **Coverage (63/100):** `python scripts/build_manifest.py` scans the proofs and prints the measured
  per-scheme distribution and the Wilson interval; it equals the count of `proven=True` rows in
  `data/sample.csv`.
- **Company similarity (*V* ≈ 0.09):** `python scripts/company_analysis.py` recomputes the scheme-level
  contingency, the bias-corrected effect size, and the per-firm bootstrap intervals.
- **Proofs:** in `proofs/`, `lake exe cache get && lake build` checks every certificate; `#print axioms`
  on the top-level theorems confirms only Lean's standard axioms are used.

## Data note (please read)

The **raw per-company problem-frequency tables are deliberately withheld** per the source platform's terms
and are *not* included here. Only *derived* statistics are released: the sample, the rule, the per-problem
family labels, and the certification manifest. The headline claims are reproducible from these derived files
plus the proof development; the underlying scrape is not redistributed.

## Authorship

Single human author, with substantial AI assistance for drafting, statistical code, and the Lean
formalization; all claims, numbers, and proofs were checked by the author. The machine-checked results are,
by construction, independently verifiable by re-running the proof development. See the paper's
"Reproducibility and AI-use disclosure" section.
