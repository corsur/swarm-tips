import Lproofs.Schemes.Fold

/-! @lc 1347 | name:Minimum Number of Steps to Make Two Strings Anagram | scheme:fold | family:hashing |
    complexity:O(n) | source:https://leetcode.com/problems/minimum-number-of-steps-to-make-two-strings-anagram/ -/

namespace LC.P1347
open Interview.Patterns

/-- Editorial frequency solution: the number of characters of `t` to replace is the size of the
    multiset difference `t \ s` (the characters `t` has that `s` cannot supply). -/
def sol (s t : List Char) : ℕ := ((t : Multiset Char) - (s : Multiset Char)).card

/-- SCHEME (fold): the character frequencies are a streaming fold. -/
theorem cls : IsFold (fun s : List Char => s.foldl (fun m c => insert c m) (0 : Multiset Char)) :=
  fold_charCount

/-- CORRECT: zero steps are needed exactly when every character of `t` is already supplied by `s`
    (for equal-length strings, when `t` is an anagram of `s`). -/
theorem corr (s t : List Char) : sol s t = 0 ↔ (t : Multiset Char) ≤ (s : Multiset Char) := by
  rw [sol, Multiset.card_eq_zero, tsub_eq_zero_iff_le]

end LC.P1347
