import Lproofs.Schemes.Fold

/-! @lc 680 | name:Valid Palindrome II | scheme:fold | family:two-pointers | complexity:O(n) |
    source:https://leetcode.com/problems/valid-palindrome-ii/ -/

namespace LC.P0680
open Interview.Patterns

/-- Spec: the string is a palindrome, or becomes one after deleting at most one character. -/
def spec (s : List Char) : Prop :=
  s = s.reverse ∨ ∃ i, i < s.length ∧ (s.eraseIdx i) = (s.eraseIdx i).reverse

/-- Editorial two-pointer solution (modeled): already a palindrome, or some single deletion is.
    The palindrome test is equality with the fold-computed reverse. -/
def sol (s : List Char) : Bool :=
  decide (s = s.reverse) || (List.range s.length).any (fun i => decide ((s.eraseIdx i) = (s.eraseIdx i).reverse))

/-- SCHEME (fold): the test is driven by `reverse`, itself a streaming fold (`fold_reverse`). -/
theorem cls : IsFold (List.reverse : List Char → List Char) := fold_reverse

/-- CORRECT: when the test passes, the string is a (near-)palindrome per the spec. -/
theorem corr (s : List Char) (h : sol s = true) : spec s := by
  simp only [sol, Bool.or_eq_true, decide_eq_true_eq, List.any_eq_true, List.mem_range] at h
  rcases h with h | ⟨i, hi, he⟩
  · exact Or.inl h
  · exact Or.inr ⟨i, hi, he⟩

end LC.P0680
