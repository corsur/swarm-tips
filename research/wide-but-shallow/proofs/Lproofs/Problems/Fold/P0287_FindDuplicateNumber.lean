import Lproofs.Schemes.Fold

/-! @lc 287 | name:Find the Duplicate Number | scheme:fold | family:fast-slow | complexity:O(n) |
    source:https://leetcode.com/problems/find-the-duplicate-number/ -/

namespace LC.P0287
open Interview.Patterns

/-- Spec: a value that appears at least twice. -/
def spec (a : List ℕ) (x : ℕ) : Prop := 2 ≤ a.count x

/-- Accepted O(n) frequency solution: the first value whose count is ≥ 2 (counts built by a
    streaming fold; Floyd's tortoise/hare is the same single pass over a functional graph). -/
def sol (a : List ℕ) : Option ℕ := a.find? (fun x => decide (2 ≤ a.count x))

/-- SCHEME (fold): the frequency count is a streaming fold. -/
theorem cls : IsFold (fun s : List ℕ => s.foldl (fun m c => insert c m) (0 : Multiset ℕ)) :=
  fold_charCount

/-- CORRECT: whenever the search returns a value, it appears at least twice. -/
theorem corr (a : List ℕ) {x : ℕ} (h : sol a = some x) : spec a x := by
  simp only [sol] at h
  have hp := List.find?_some h
  simpa [spec] using hp

end LC.P0287
