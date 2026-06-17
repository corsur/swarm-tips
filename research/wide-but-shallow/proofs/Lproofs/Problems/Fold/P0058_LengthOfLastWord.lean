import Lproofs.Problems.Fold.P0151_ReverseWords

/-! @lc 58 | name:Length of Last Word | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/length-of-last-word/

    The answer is the length of the last word (split on spaces, dropping empties). Correctness:
    that length is zero exactly when the string has no words at all — every actual word is
    nonempty. Reuses the verified word-splitter from `LC.P0151`. -/

namespace LC.P0058
open Interview.Patterns

def sol (s : List Char) : ℕ := ((LC.P0151.words s).getLastD []).length

/-- SCHEME (fold): the answer reads off the verified word-splitter `LC.P0151.splitSp`, which is a
    genuine right fold over the characters (not a proxy). -/
theorem cls : IsRightFold LC.P0151.splitSp := LC.P0151.cls

/-- CORRECT: the last-word length is zero exactly when there are no words. -/
theorem corr (s : List Char) : sol s = 0 ↔ LC.P0151.words s = [] := by
  unfold sol
  cases h : LC.P0151.words s with
  | nil => simp
  | cons w ws =>
    simp only [reduceCtorEq, iff_false, List.length_eq_zero_iff]
    intro hempty
    have hmem : (w :: ws).getLastD [] ∈ w :: ws := by
      rw [List.getLastD_eq_getLast?, List.getLast?_eq_getLast (by simp), Option.getD_some]
      exact List.getLast_mem _
    have hmem2 : (w :: ws).getLastD [] ∈ LC.P0151.words s := by rw [h]; exact hmem
    have hne := List.of_mem_filter hmem2
    rw [hempty] at hne
    simp at hne

end LC.P0058
