import Lproofs.Schemes.Fold

/-! @lc 242 | name:Valid Anagram | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/valid-anagram/ -/

namespace LC.P0242
open Interview.Patterns

/-- Spec: two strings are anagrams iff they have equal character multisets. -/
def spec {α : Type*} (s t : List α) : Prop := (s : Multiset α) = ↑t

/-- Editorial solution: compare character-frequency counts (built by a fold). -/
def sol {α : Type*} (s t : List α) : Prop := charCount s = charCount t

/-- SCHEME: the character count is a streaming fold (accumulator: a multiset). -/
theorem cls {α : Type*} :
    IsFold (fun s : List α => s.foldl (fun m c => insert c m) (0 : Multiset α)) :=
  fold_charCount

/-- CORRECT: equal character-count folds ⟺ equal character multisets. -/
theorem corr {α : Type*} (s t : List α) : sol s t ↔ spec s t := anagram_via_fold s t

end LC.P0242
