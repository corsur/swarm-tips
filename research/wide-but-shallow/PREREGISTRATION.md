# Pre-registration — fresh-seed certification round

**Committed before drawing the sample or computing any certified count.** This document fixes the
protocol in advance so the headline coverage number cannot be a selected ("shopped") seed.

## Why

An adversarial re-review (2026-06-19) established that the original headline sample (`seed=7`) is the
single highest-scoring draw out of 2000 seeds: with the certificates as they then stood, seed 7
yielded 76/100 genuine certs while the mean random draw yields ~33.5 (5–95% [26,41]). That is the
expected signature of fixing one seed and grinding *that* sample to completion — not fraud — but it
means the proof-gated 76/100 measures the completeness of the one worked sample, not an unbiased
population coverage rate. The seed-stable quantity (the fixed family→scheme rule's in-scheme rate,
~75%, 5–95% [68,81]) is unaffected and remains the empirical "four ideas" claim.

To report an **unbiased** proof-gated coverage number, we pre-register a new seed and will certify its
sample to completion, reporting whatever results.

## Protocol (fixed in advance)

1. **Seed:** `20260619` (today's date, YYYYMMDD). Chosen without computing its certified count, and
   outside the 0–1999 range whose scores were swept during the review.
2. **Draw:** `sample.py --seed 20260619 --n 100` over the full 769-problem population
   (`labels.csv`, sorted by integer id), identical mechanism to the original.
3. **Certify to completion:** for every in-scheme problem in the drawn sample, attempt a
   problem-specific certificate to the same standard used elsewhere (`cls` scheme membership of a
   concrete solution + a `corr` that references the concrete problem, not a scheme-generic relation;
   no `sorry`; standard axioms only). Do **not** stop early on hard problems selectively.
4. **Leave-out rule:** a problem stays uncounted only if it genuinely resists a problem-specific
   certificate in a single self-contained file (e.g. full optimization optimality or a data-structure
   query invariant), and every such problem is listed explicitly with its reason.
5. **Report:** the genuine count on this sample becomes the headline, with its Wilson interval. No
   seed re-selection: the number this seed yields is the number reported.

## Commitment

The reported headline will be whatever `seed=20260619`, certified to completion under the rule above,
yields — higher or lower than the prior 76. This file is committed before any of that work begins.
