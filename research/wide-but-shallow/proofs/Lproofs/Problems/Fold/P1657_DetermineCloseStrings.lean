import Lproofs.Schemes.Fold

/-! @lc 1657 | name:Determine if Two Strings Are Close | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/determine-if-two-strings-are-close/ -/

namespace LC.P1657
open Interview.Patterns

/-- Spec: two strings are "close" iff they have the same set of distinct characters and the same
    multiset of character frequencies (swaps permute positions; transforms permute which letter has
    which frequency). -/
def spec (a b : List Char) : Prop :=
  a.toFinset = b.toFinset ∧ a.toFinset.val.map a.count = b.toFinset.val.map b.count

/-- Editorial frequency solution: compare distinct-character sets and frequency multisets (both
    built by a streaming fold). -/
def sol (a b : List Char) : Bool :=
  decide (a.toFinset = b.toFinset ∧ a.toFinset.val.map a.count = b.toFinset.val.map b.count)

/-- SCHEME (fold): the character frequencies are a streaming fold. -/
theorem cls : IsFold (fun xs : List Char => xs.foldl (fun m c => insert c m) (0 : Multiset Char)) :=
  fold_charCount

/-- CORRECT: the boolean answer matches the closeness predicate. -/
theorem corr (a b : List Char) : sol a b = true ↔ spec a b := by
  simp [sol, spec]

end LC.P1657
