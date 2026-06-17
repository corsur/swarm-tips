import Lproofs.Schemes.Fold

/-! @lc 203 | name:Remove Linked List Elements | scheme:fold | family:linked-list | complexity:O(n) |
    source:https://leetcode.com/problems/remove-linked-list-elements/ -/

namespace LC.P0203
open Interview.Schemes

/-- Spec: the list with every node equal to `val` removed (order preserved). -/
def spec (val : ℕ) (xs : List ℕ) : List ℕ := xs.filter (fun x => x != val)

/-- Editorial single-pass solution: rebuild keeping non-`val` nodes (accumulator = kept prefix). -/
def sol (val : ℕ) (xs : List ℕ) : List ℕ :=
  xs.foldl (fun a x => if x != val then a ++ [x] else a) []

/-- SCHEME: the traversal is a streaming fold. -/
theorem cls (val : ℕ) : IsFold (sol val) := ⟨_, [], fun _ => rfl⟩

/-- CORRECT: the fold computes the filtered list. -/
theorem corr (val : ℕ) (xs : List ℕ) : sol val xs = spec val xs := by
  rw [sol, spec, foldl_filter_append, List.nil_append]

end LC.P0203
