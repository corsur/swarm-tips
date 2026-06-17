import Lproofs.Schemes.Fold

/-! @lc 416 | name:Partition Equal Subset Sum | scheme:dp | family:dp-linear | complexity:O(n·sum) |
    source:https://leetcode.com/problems/partition-equal-subset-sum/

    The array splits into two equal-sum halves iff some subset sums to `total/2`. CLASSIFICATION: the
    accepted subset-sum DP is a recursive decomposition — for each element, either include it (recurse
    on the rest with `t − x`) or drop it (recurse with `t`). NON-VACUITY: we prove every enumerated
    subset genuinely sums to the target, pinning the decomposition to real subproblem work. We certify
    the decomposition + soundness. -/

namespace LC.P0416

/-- Subsets of `l` (no reuse) summing to `t` — the subset-sum recursive decomposition. -/
def subsetsTo : List ℕ → ℕ → List (List ℕ)
  | [], t => if t = 0 then [[]] else []
  | x :: xs, t =>
    (if x ≤ t then (subsetsTo xs (t - x)).map (x :: ·) else []) ++ subsetsTo xs t

/-- SCHEME (dp / recursive decomposition): include the head (recurse with `t − x`, prepend it) or
    drop it (recurse with `t`) — the genuine subset-sum split. -/
theorem cls (x : ℕ) (xs : List ℕ) (t : ℕ) :
    subsetsTo (x :: xs) t =
      (if x ≤ t then (subsetsTo xs (t - x)).map (x :: ·) else []) ++ subsetsTo xs t :=
  rfl

/-- NON-VACUITY (soundness): every enumerated subset sums to the target. -/
theorem corr : ∀ (l : List ℕ) (t : ℕ) (s : List ℕ), s ∈ subsetsTo l t → s.sum = t := by
  intro l
  induction l with
  | nil =>
    intro t s hs
    cases t with
    | zero => simp only [subsetsTo, if_pos, List.mem_singleton] at hs; subst hs; rfl
    | succ k => simp [subsetsTo] at hs
  | cons x xs ih =>
    intro t s hs
    rw [cls, List.mem_append] at hs
    rcases hs with h | h
    · split at h
      · rw [List.mem_map] at h
        obtain ⟨s', hs', rfl⟩ := h
        have := ih (t - x) s' hs'
        rw [List.sum_cons, this]; omega
      · simp at h
    · exact ih t s h

end LC.P0416
