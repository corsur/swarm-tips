import Lproofs.Schemes.Fold

/-! @lc 567 | name:Permutation in String | scheme:fold | family:sliding-window | complexity:O(n) |
    source:https://leetcode.com/problems/permutation-in-string/ -/

namespace LC.P0567
open Interview.Patterns

/-- Spec: some length-`|s1|` window of `s2` has the same character multiset as `s1` (i.e. contains
    a permutation of `s1`). -/
def spec (s1 s2 : List Char) : Prop :=
  ∃ i, i + s1.length ≤ s2.length ∧ (↑((s2.drop i).take s1.length) : Multiset Char) = ↑s1

/-- Editorial sliding-window: scan every window, comparing character counts. -/
def sol (s1 s2 : List Char) : Bool :=
  (List.range (s2.length + 1)).any
    (fun i => decide (i + s1.length ≤ s2.length ∧ (↑((s2.drop i).take s1.length) : Multiset Char) = ↑s1))

/-- SCHEME (fold): the window character-count is a streaming fold. -/
theorem cls : IsFold (fun s : List Char => s.foldl (fun m c => insert c m) (0 : Multiset Char)) :=
  fold_charCount

/-- CORRECT: the boolean answer matches "a permutation of `s1` occurs as a window of `s2`". -/
theorem corr (s1 s2 : List Char) : sol s1 s2 = true ↔ spec s1 s2 := by
  simp only [sol, List.any_eq_true, List.mem_range, decide_eq_true_eq]
  constructor
  · rintro ⟨i, _, hle, hms⟩; exact ⟨i, hle, hms⟩
  · rintro ⟨i, hle, hms⟩; exact ⟨i, by omega, hle, hms⟩

end LC.P0567
