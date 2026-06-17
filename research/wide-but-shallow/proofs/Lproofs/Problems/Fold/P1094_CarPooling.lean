import Lproofs.Schemes.Fold

/-! @lc 1094 | name:Car Pooling | scheme:fold | family:diff-array | complexity:O(n + maxPos) |
    source:https://leetcode.com/problems/car-pooling/ -/

namespace LC.P1094
open Interview.Patterns

/-- `deltas` is the difference array (`+passengers` at each pickup position, `-passengers` at each
    drop-off). Occupancy at any point is a prefix sum of `deltas`. -/
def spec (deltas : List ℤ) (cap : ℤ) : Prop := ∀ k ≤ deltas.length, (deltas.take k).sum ≤ cap

/-- Editorial difference-array sweep: occupancy (running prefix sum) never exceeds capacity. -/
def sol (deltas : List ℤ) (cap : ℤ) : Bool :=
  (List.range (deltas.length + 1)).all (fun k => decide ((deltas.take k).sum ≤ cap))

/-- SCHEME (fold): the occupancy is a running prefix-sum scan. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (· + ·) 0) := fold_prefixSum

/-- CORRECT: the answer is true iff every prefix occupancy stays within capacity. -/
theorem corr (deltas : List ℤ) (cap : ℤ) : sol deltas cap = true ↔ spec deltas cap := by
  simp only [sol, spec, List.all_eq_true, List.mem_range, decide_eq_true_eq]
  constructor
  · intro h k hk; exact h k (Nat.lt_succ_of_le hk)
  · intro h k hk; exact h k (Nat.le_of_lt_succ hk)

end LC.P1094
