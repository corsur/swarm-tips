import Lproofs.Schemes.Fold

/-! @lc 125 | name:Valid Palindrome | scheme:fold | family:two-pointers | complexity:O(n) |
    source:https://leetcode.com/problems/valid-palindrome/ -/

namespace LC.P0125
open Interview.Patterns

/-- Spec: the sequence reads the same forwards and reversed. (The full problem's
    filter-to-alphanumeric preprocessing is a prior fold; the palindrome core is modeled here.) -/
def spec {α : Type*} (s : List α) : Prop := s = s.reverse

/-- Editorial two-pointer solution, modeled as equality with the fold-computed reverse. -/
def sol {α : Type*} [DecidableEq α] (s : List α) : Bool := isPalindrome s

/-- SCHEME: the test is driven by `reverse`, itself a streaming fold (`fold_reverse`). -/
theorem cls {α : Type*} : IsFold (List.reverse : List α → List α) := fold_reverse

/-- CORRECT: the test holds iff the sequence equals its reverse. -/
theorem corr {α : Type*} [DecidableEq α] (s : List α) : sol s = true ↔ spec s :=
  palindrome_via_fold s

end LC.P0125
