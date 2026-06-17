import Lproofs.Schemes.Fold

/-! @lc 252 | name:Meeting Rooms | scheme:fold | family:diff-array | complexity:O(n log n) |
    source:https://leetcode.com/problems/meeting-rooms/

    A single person can attend all meetings iff no two overlap, i.e. at most one meeting is ever in
    progress. Sweeping a time-sorted difference array (`+1` at each start, `-1` at each end), the
    number in progress is a prefix sum; "attend all" holds exactly when that occupancy never exceeds
    one. (This is the same prefix-sum sweep as Car Pooling / Meeting Rooms II with capacity 1.) -/

namespace LC.P0252
open Interview.Patterns

/-- `deltas` is the time-sorted difference array. Occupancy after the first `k` events is the prefix
    sum; a person can attend all meetings iff occupancy never exceeds one. -/
def spec (deltas : List ℤ) : Prop := ∀ k ≤ deltas.length, (deltas.take k).sum ≤ 1

/-- Editorial difference-array sweep: the running occupancy never exceeds one. -/
def sol (deltas : List ℤ) : Bool :=
  (List.range (deltas.length + 1)).all (fun k => decide ((deltas.take k).sum ≤ 1))

/-- SCHEME (fold): the occupancy is a running prefix-sum scan over the difference array. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (· + ·) 0) := fold_prefixSum

/-- CORRECT: a person can attend all meetings iff every prefix occupancy stays within one. -/
theorem corr (deltas : List ℤ) : sol deltas = true ↔ spec deltas := by
  simp only [sol, spec, List.all_eq_true, List.mem_range, decide_eq_true_eq]
  constructor
  · intro h k hk; exact h k (Nat.lt_succ_of_le hk)
  · intro h k hk; exact h k (Nat.le_of_lt_succ hk)

end LC.P0252
