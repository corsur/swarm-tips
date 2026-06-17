import Lproofs.Schemes.Fold

/-! @lc 39 | name:Combination Sum | scheme:dp | family:backtracking | complexity:exponential |
    source:https://leetcode.com/problems/combination-sum/

    Enumerate every multiset of (reusable) candidates summing to `target`. CLASSIFICATION: the accepted
    backtracking is a recursive decomposition — at each step either take the head candidate `c` (recurse
    on the same list with `target - c`, prepending `c`) or drop it (recurse on the tail). NON-VACUITY:
    we prove every enumerated combination genuinely sums to `target`, pinning the decomposition to real
    subproblem work. We certify the decomposition + soundness, not completeness/optimality. -/

namespace LC.P0039

/-- Combinations summing to `t` from `cands` (with reuse); `fuel` bounds the recursion depth. -/
def combos : ℕ → List ℕ → ℕ → List (List ℕ)
  | 0, _, _ => []
  | _ + 1, _, 0 => [[]]
  | _ + 1, [], _ => []
  | fuel + 1, c :: cs, t + 1 =>
    (if c ≤ t + 1 then (combos fuel (c :: cs) (t + 1 - c)).map (c :: ·) else []) ++
      combos fuel cs (t + 1)

/-- SCHEME (dp / recursive decomposition): take the head candidate (recurse with `t - c`, prepend it)
    or drop it (recurse on the tail) — the genuine subproblem split. -/
theorem cls (fuel : ℕ) (c : ℕ) (cs : List ℕ) (t : ℕ) :
    combos (fuel + 1) (c :: cs) (t + 1) =
      (if c ≤ t + 1 then (combos fuel (c :: cs) (t + 1 - c)).map (c :: ·) else []) ++
        combos fuel cs (t + 1) :=
  rfl

/-- NON-VACUITY (soundness): every enumerated combination sums to the target. -/
theorem corr : ∀ (fuel : ℕ) (cands : List ℕ) (t : ℕ) (combo : List ℕ),
    combo ∈ combos fuel cands t → combo.sum = t := by
  intro fuel
  induction fuel with
  | zero => intro cands t combo hmem; simp [combos] at hmem
  | succ f ih =>
    intro cands t combo hmem
    rcases t with _ | t
    · simp only [combos, List.mem_singleton] at hmem; subst hmem; rfl
    · rcases cands with _ | ⟨c, cs⟩
      · simp [combos] at hmem
      · rw [cls, List.mem_append] at hmem
        rcases hmem with h | h
        · split at h
          · rw [List.mem_map] at h
            obtain ⟨combo', hmem', rfl⟩ := h
            have hc' := ih (c :: cs) (t + 1 - c) combo' hmem'
            rw [List.sum_cons, hc']
            omega
          · simp at h
        · exact ih cs (t + 1) combo h

end LC.P0039
