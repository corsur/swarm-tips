import Lproofs.Schemes.Fold

/-! @lc 27 | name:Remove Element | scheme:fold | family:two-pointers | complexity:O(n) |
    source:https://leetcode.com/problems/remove-element/ -/

namespace LC.P0027
open Interview.Schemes

/-- Spec: the array with every occurrence of `val` removed (order preserved). -/
def spec (val : ℕ) (xs : List ℕ) : List ℕ := xs.filter (fun x => x != val)

/-- Editorial two-pointer in-place compaction: the write index is the fold accumulator. -/
def sol (val : ℕ) (xs : List ℕ) : List ℕ :=
  xs.foldl (fun a x => if x != val then a ++ [x] else a) []

/-- SCHEME: the compaction is a streaming fold. -/
theorem cls (val : ℕ) : IsFold (sol val) := ⟨_, [], fun _ => rfl⟩

/-- CORRECT: the fold computes the filtered array. -/
theorem corr (val : ℕ) (xs : List ℕ) : sol val xs = spec val xs := by
  rw [sol, spec, foldl_filter_append, List.nil_append]

end LC.P0027
