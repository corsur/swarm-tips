import Lproofs.Schemes.Fold

/-! @lc 268 | name:Missing Number | scheme:fold | family:math-bit | complexity:O(n) |
    source:https://leetcode.com/problems/missing-number/ -/

namespace LC.P0268
open Interview.Patterns

/-- Spec: the answer is in range `[0, n]` and is absent from the array (the missing number;
    given `a` is `n` distinct values from `0..n`, this pins it uniquely). -/
def spec (a : List ℕ) (x : ℕ) : Prop := x ∉ a ∧ x ≤ a.length

/-- Editorial O(n) membership solution: scan `0..n` for the first value absent from the array
    (membership built by a streaming fold — the same set the XOR/sum editorials summarise). -/
def sol (a : List ℕ) : Option ℕ := (List.range (a.length + 1)).find? (fun x => decide (x ∉ a))

/-- SCHEME (fold): the membership/seen-set is a streaming fold. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl (fun s x => insert x s) (∅ : Finset ℕ)) :=
  fold_seenSet

/-- CORRECT: whenever the search returns a value, it is in range and absent from the array. -/
theorem corr (a : List ℕ) {x : ℕ} (h : sol a = some x) : spec a x := by
  simp only [sol] at h
  have hp := List.find?_some h
  have hm := List.mem_of_find?_eq_some h
  rw [List.mem_range] at hm
  exact ⟨by simpa using hp, by omega⟩

end LC.P0268
