import Lproofs.Schemes.Fold

/-! @lc 253 | name:Meeting Rooms II | scheme:fold | family:diff-array | complexity:O(n log n) |
    source:https://leetcode.com/problems/meeting-rooms-ii/

    Sort the start/end events and sweep a difference array (`+1` at each meeting start, `-1` at each
    end). The number of rooms in use at any moment is a prefix sum of the deltas; `rooms` rooms
    suffice exactly when no prefix occupancy ever exceeds `rooms`. -/

namespace LC.P0253
open Interview.Patterns

/-- `deltas` is the time-sorted difference array (`+1` at a meeting start, `-1` at an end). Room
    occupancy after the first `k` events is the prefix sum of `deltas`. -/
def spec (deltas : List ℤ) (rooms : ℤ) : Prop := ∀ k ≤ deltas.length, (deltas.take k).sum ≤ rooms

/-- Editorial difference-array sweep: occupancy (running prefix sum) never exceeds the room count. -/
def sol (deltas : List ℤ) (rooms : ℤ) : Bool :=
  (List.range (deltas.length + 1)).all (fun k => decide ((deltas.take k).sum ≤ rooms))

/-- SCHEME (fold): the occupancy is a running prefix-sum scan over the difference array. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (· + ·) 0) := fold_prefixSum

/-- CORRECT: `rooms` rooms suffice iff every prefix occupancy stays within `rooms`. -/
theorem corr (deltas : List ℤ) (rooms : ℤ) : sol deltas rooms = true ↔ spec deltas rooms := by
  simp only [sol, spec, List.all_eq_true, List.mem_range, decide_eq_true_eq]
  constructor
  · intro h k hk; exact h k (Nat.lt_succ_of_le hk)
  · intro h k hk; exact h k (Nat.le_of_lt_succ hk)

end LC.P0253
