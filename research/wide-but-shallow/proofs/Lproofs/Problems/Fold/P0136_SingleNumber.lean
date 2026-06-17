import Lproofs.Schemes.Fold

/-! @lc 136 | name:Single Number | scheme:fold | family:math-bit | complexity:O(n) |
    source:https://leetcode.com/problems/single-number/ -/

namespace LC.P0136
open Interview.Patterns

/-- Spec: an element that appears exactly once. -/
def spec (a : List ℕ) (x : ℕ) : Prop := a.count x = 1

/-- Accepted O(n) frequency solution: the first element whose count is one (frequencies are
    built by a streaming fold — the hashing/counting structure; the XOR editorial is the same
    fold over the XOR monoid). -/
def sol (a : List ℕ) : Option ℕ := a.find? (fun x => a.count x == 1)

/-- SCHEME (fold): the frequency count is a streaming fold (accumulator: a multiset). -/
theorem cls : IsFold (fun s : List ℕ => s.foldl (fun m c => insert c m) (0 : Multiset ℕ)) :=
  fold_charCount

/-- CORRECT: whenever the search returns an element, that element appears exactly once. -/
theorem corr (a : List ℕ) {x : ℕ} (h : sol a = some x) : spec a x := by
  simp only [sol] at h
  have hp := List.find?_some h
  simpa [spec] using hp

end LC.P0136
