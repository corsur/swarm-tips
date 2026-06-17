import Lproofs.Schemes.Fold

/-! @lc 387 | name:First Unique Character in a String | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/first-unique-character-in-a-string/ -/

namespace LC.P0387
open Interview.Patterns

/-- Spec: a character that occurs exactly once in the string. -/
def spec (s : List Char) (c : Char) : Prop := s.count c = 1

/-- Editorial frequency solution: build the character-frequency multiset with a streaming fold
    (`charCount`), then return the first character whose frequency is one. The fold-built
    `charCount` is the solution's actual working structure. -/
def sol (s : List Char) : Option Char :=
  s.find? (fun c => decide ((charCount s).count c = 1))

/-- SCHEME (fold): the frequency structure `sol` reads is the streaming `charCount` fold. -/
theorem cls : IsFold (fun s : List Char => s.foldl (fun m c => insert c m) (0 : Multiset Char)) :=
  fold_charCount

/-- CORRECT: whenever the search returns a character, it occurs exactly once. -/
theorem corr (s : List Char) {c : Char} (h : sol s = some c) : spec s c := by
  simp only [sol] at h
  have hp := List.find?_some h
  rw [decide_eq_true_iff, charCount_eq, Multiset.coe_count] at hp
  exact hp

end LC.P0387
