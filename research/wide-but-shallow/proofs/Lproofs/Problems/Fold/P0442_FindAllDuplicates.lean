import Lproofs.Schemes.Fold

/-! @lc 442 | name:Find All Duplicates in an Array | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/find-all-duplicates-in-an-array/ -/

namespace LC.P0442
open Interview.Patterns

/-- Accepted O(n) frequency solution: the distinct values whose count is ≥ 2 (counts built by a
    streaming fold). -/
def sol (a : List ℕ) : List ℕ := a.dedup.filter (fun x => decide (2 ≤ a.count x))

/-- Spec: each reported value appears at least twice in the array. -/
def spec (a : List ℕ) (y : ℕ) : Prop := 2 ≤ a.count y

/-- SCHEME (fold): the frequency count is a streaming fold. -/
theorem cls : IsFold (fun s : List ℕ => s.foldl (fun m c => insert c m) (0 : Multiset ℕ)) :=
  fold_charCount

/-- CORRECT: every reported value genuinely appears at least twice. -/
theorem corr (a : List ℕ) {y : ℕ} (h : y ∈ sol a) : spec a y := by
  simp only [sol, List.mem_filter, decide_eq_true_eq] at h
  exact h.2

end LC.P0442
