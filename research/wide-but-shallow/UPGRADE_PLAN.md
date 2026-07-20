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

## Derived worklist (Step 0 DONE 2026-07-19, from the corr statements)
Tier counts as-derived: EXACT/full ≈ 36 (incl. all bisection IsLeast, relaxation spec-equality,
iff-certs, closed forms, round-trips), exact-IDENTITY ≈ 15 (recurrences, telescoping, conservation
where the def IS the spec), ONE-DIRECTIONAL/upgradable = 18 below.

**Band A — near-free converses:**
- [x] 1268 completeness of the filter (prefixed product ∈ sol) — mem_filter
- [x] 1492 completeness (0<d ∣ n, 0<n → d ∈ sol n) — mem_filter + le_of_dvd
- [x] 314 converse (every tree value gets a column: x ∈ inorder t → ∃ col, (col,x) ∈ sol t c)
- [x] 536 converse (sol t ⊆ inorder t — same induction as corr)
- [x] 939 accept-completeness (4 corners present ∧ nondegenerate → accept = true; near-rfl)
- [x] 3043 maximality (any common prefix of xs,ys is a prefix of sol xs ys)
- [x] 88 sorted-merge upgrade (Sorted a → Sorted b → Sorted (sol a b) ∧ perm (a++b))
- [x] 101 bonus: inorder (sol t) = (inorder t).reverse (mirror effect, clone of 226)

**Band B — template work:**
- [x] 20 strict checker + Dyck-condition iff (no import needed — spec matches Mathlib DyckWord condition verbatim)
- [x] 111 exact minDepth = min leaf-path length
- [x] 120 sol = min over all real descents (achievable+optimal template)
- [x] 71 canonical-path semantics
- [ ] 208 full trie spec (contains ↔ inserted-word set) — medium
- [ ] 1166 same shape as 208 — medium
- [x] 759 emitted-gap characterization ((g1,g2) ∈ sol l ↔ consecutive pair boundaries)
- [x] 3466 optimality over switch-sequences (achievable+optimal)
- [x] 3742 optimality over move-sequences (achievable+optimal)
- [ ] 1823 exact Josephus process spec — HARD-B (elimination-process model ~150 ln; park OK)

**Band C — citation-backed (bib pre-staged in paper.tex):**
- [x] 142 docstring: Phase-2/full correctness cited — AFP TortoiseHare (Gammie 2015)
- [x] 516 docstring: optimality core cited — afp_dp Longest_Common_Subsequence; LPS(s)=LCS(s,rev s)

**Band D — hard core (park with notes unless a reframe appears):**
- [ ] 188 k-transaction optimality (no citation exists; 200-400 ln; park expected)
- [ ] 543 exact tree diameter (no citation exists; path formalization; park expected)

Not upgraded on purpose (def IS the spec, or corr already the full law): 94, 104, 209, 541,
647, 736, 1239, 1385, 1387, 1868, 1915, 2149, 2423, 2791, 2965 — identity tier, disclosed.

## Completed
- [x] Turn 6 (Band B flagship): 20 — strict checker `check` (Option-depth fold, pop-on-empty fails), `check_iff_balanced` (accepts ↔ Mathlib's Dyck condition: prefix-closes ≤ opens ∧ totals equal), `check_sol_bridge` (lenient stack = replicate of strict depth), strict ground instances incl. the ")" rejection the lenient model missed. THE modelling-gap catch of the run — cite in paper §4 as evidence the upgrade process has teeth.
- [x] Turn 5 (Band B): 3742 play/achievable/optimal — sol = exact max affordable path score over move-strategies (boundary-0 branches mirrored in play). Gotcha: set-folding (set b' := …) desyncs with simp unfolds — inline the expression; rewrite h (if_pos/heq) BEFORE max_eq_* selection.
- NOTE for 20 (next): current lenient stack (tail on empty) means sol s = [] does NOT imply balanced (e.g. ")"). Upgrade = strict checker (Option (List Char) fold, pop-on-empty ⇒ none) + own inductive Balanced grammar + iff; adjust cls conj + vec accordingly. DyckWord import optional — own Balanced is cleaner.
- [x] Turn 4 (Band B×3): 759 sol_eq_zip (gaps = exactly adjacent-pair boundaries); 71 three laws + laws_unique (initial-algebra full spec); 3466 play/achievable/optimal (sol = exact max over switch-strategies). Gotcha: sw-1 leaves `s+1-1` unreduced — simp only [Nat.add_sub_cancel] after if_pos.
- [x] Turn 3 (Band B×2): 120 achievable+optimal (sol = exact min over all descents, pathSum choice-lists); 111 exact (sol ∈ leafDepths ∧ least — genuine shortest root-to-leaf). Gotchas: simp only [leafDepths] unfolds recursively — use single rw [leafDepths] + explicit if_neg hcond; add_le_add le_rfl for c+a ≤ c+b.
- [x] Turn 2 (Band A closed): 939 accept-completeness, 3043 maximality (sol = THE longest common prefix), 88 merge_perm + merge_sorted (full functional spec), 101 mirror-inorder effect. Gotchas: List.Sorted is gone in this Mathlib — use List.Pairwise (· ≤ ·) + pairwise_cons; sol-unfold turns `a = a` conditions into `if True` — close with if_pos trivial.
- [x] Turn 1 (Step 0 + Band C + Band A×4): 1268, 1492, 314, 536 upgraded to full characterizations; 142/516 citation-noted.
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
