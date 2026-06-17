import Lproofs.Schemes.Fold

/-! @lc 383 | name:Ransom Note | scheme:fold | family:hashing | complexity:O(n+m) |
    source:https://leetcode.com/problems/ransom-note/ -/

namespace LC.P0383
open Interview.Patterns

/-- Spec: the note can be built from the magazine iff its character multiset is contained in
    the magazine's (each letter used at most as often as it occurs). -/
def spec (note mag : List Char) : Prop := (note : Multiset Char) ≤ ↑mag

/-- Editorial frequency solution: compare character counts (built by a streaming fold). -/
def sol (note mag : List Char) : Bool := decide ((note : Multiset Char) ≤ ↑mag)

/-- SCHEME (fold): the character-frequency count is a streaming fold. -/
theorem cls : IsFold (fun s : List Char => s.foldl (fun m c => insert c m) (0 : Multiset Char)) :=
  fold_charCount

/-- CORRECT: the boolean answer matches multiset containment. -/
theorem corr (note mag : List Char) : sol note mag = true ↔ spec note mag := by
  simp [sol, spec]

end LC.P0383
