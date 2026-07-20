# One-directional → full-correctness upgrade run (worklist + state)

Goal: close the certificate-strength gap for the one-directional tier of the 69 in-sample
certificates — by proof where tractable, by citation to existing machine-checked work where it
exists (the LC 1489/1192 precedent), with an honest parked list for what resists. Then recount
the §4 tiers and simplify the paper.

**Invariants for every turn (non-negotiable):**
- `lake build` green, `lake exe gate` = 69/69 in-sample, no `sorry`, axioms ⊆ {propext, Quot.sound, Classical.choice}.
- Never fabricate a spec; park a cert after 2 failed attempts with a written note here.
- Each green batch: rsync `lproofs/` → `~/swarm/swarm-tips-repo/research/wide-but-shallow/proofs/`, commit direct to main, push.
- Update the checkboxes + notes in THIS file every turn (it is the loop's memory).

## Step 0 (first turn): reclassify from the files
Derive the honest current tier of every in-sample cert from its `corr` statement (exact answer /
exact identity / one-directional). Record the list below under "Derived worklist". The 18-count
in the paper is from June; the definitive list comes from the files.

## Citation/reuse reconnaissance (DONE 2026-07-19 — use these, do not re-search)
- **LC 142 (Floyd cycle detection)** → CITE AFP "The Tortoise and the Hare Algorithm"
  (Peter Gammie, 2015): full algorithm verified incl. locating the cycle start (our missing
  Phase-2 direction). Add `afp_tortoisehare` bib entry + docstring note; mark cert
  citation-backed for the optimality direction.
- **LC 516 (Longest Palindromic Subsequence)** → CITE AFP Monad_Memo_DP (already in the
  bibliography as `afp_dp`): its `Longest_Common_Subsequence` theory verifies the optimal LCS
  DP; LPS(s) = LCS(s, reverse s). Optionally prove the small reduction lemma in-repo; the
  optimality core is cited.
- **LC 20 (Valid Parentheses)** → IMPORT `Mathlib.Combinatorics.Enumerative.DyckWord`
  (structure `DyckWord`, `DyckStep`): prove `run s = [] ↔` the bracket string is a Dyck word.
  Reuse beats citation here.
- **LC 543 (tree diameter)** — nothing citable found (Mathlib `SimpleGraph.Diam` is a
  definition, not the two-BFS algorithm). Hard core: prove exact max-leaf-path or park.
- **LC 188 (k-transaction stock)** — nothing citable found anywhere. Hard core: the
  achievable+optimal grind (historically abandoned twice at ~200-400 lines) or park.

## Upgrade bands (initial estimate — refine in Step 0)
- **Band A, near-free converses (~30-60 ln):** 1268 (filter completeness), 314 (every tree
  value appears), 536 (converse inclusion), 88 (sorted-merge: sorted + permutation).
- **Band B, template work (~80-150 ln):** 20 (DyckWord iff), 111 (exact shortest leaf path),
  120 (min over all descents — achievable+optimal template), 71 (canonical-path semantics),
  3466 / 3742 (optimality over move sequences), 1823 (exact Josephus spec?), 541/2149/1868
  (upgrade conservation to full output spec where cheap).
- **Band C, citation-backed:** 142 (AFP TortoiseHare), 516 (afp_dp LCS + reduction note).
- **Band D, hard core (park with notes unless a reframe appears):** 188, 543.

## Derived worklist (fill in Step 0)
- [ ] …

## Completed
- [x] 139 Word Break → FULL characterization (`strip_complete` + `complete`, commit b2c5623).

## Closing steps (after the worklist is exhausted)
1. Recount tiers from the files; update §4's 29/22/18 sentence and the abstract's strength
   clause to the new honest numbers. (The `afp_tortoisehare` bib entry and the §4
   citation-hook sentence are ALREADY in paper.tex — pre-staged 2026-07-19; just keep the
   counts honest and note the citations in the two cert docstrings.)
2. Paper simplification pass: compress §4 to (certificate definition ¶, falsifiability
   sentence, pre-registration ¶, editorial ¶, one-clause tier note); dedupe gate text vs
   Reproducibility section; trim Threats overlap. Keep the pre-registration story untouched.
3. `./build.sh all`, sync paper+README+proofs to the repo, final commit+push.
4. Update `~/.claude/.../memory/project_leetcode_algebra_paper.md` with the outcome, then STOP.
