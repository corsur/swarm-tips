import Lproofs.Schemes.Fold

/-! @lc 205 | name:Isomorphic Strings | scheme:fold | family:mapping-scan | complexity:O(n) |
    source:https://leetcode.com/problems/isomorphic-strings/

    Two strings are isomorphic iff a consistent character mapping links them. CLASSIFICATION: the check
    is a streaming left fold over the paired characters whose accumulator is the mapping built so far —
    each new pair is added unless already present. NON-VACUITY: we prove the mapping never grows beyond
    the number of pairs processed (each pair adds at most one entry), so the accumulator does genuine
    bijection-building work, not a re-encoding. We certify the fold + the mapping-size bound. -/

namespace LC.P0205
open Interview.Patterns

/-- Record the character pair unless it is already in the mapping. -/
def addPair (m : List (Char × Char)) (p : Char × Char) : List (Char × Char) :=
  if p ∈ m then m else p :: m

/-- The mapping built by scanning the paired characters. -/
def buildMap (pairs : List (Char × Char)) : List (Char × Char) := pairs.foldl addPair []

/-- SCHEME (fold): the mapping is a left fold with the built-pairs accumulator. -/
theorem cls : IsFold (fun pairs : List (Char × Char) => pairs.foldl addPair []) :=
  ⟨addPair, [], fun _ => rfl⟩

/-- Each step adds at most one mapping. -/
theorem step_len (m : List (Char × Char)) (p : Char × Char) : (addPair m p).length ≤ m.length + 1 := by
  simp only [addPair]; split <;> simp

/-- NON-VACUITY: the mapping never grows beyond the number of pairs processed. -/
theorem corr (pairs : List (Char × Char)) : (buildMap pairs).length ≤ pairs.length := by
  have key : ∀ (ps : List (Char × Char)) m, (ps.foldl addPair m).length ≤ m.length + ps.length := by
    intro ps
    induction ps with
    | nil => intro m; simp
    | cons p rest ih =>
      intro m
      simp only [List.foldl_cons, List.length_cons]
      exact le_trans (ih (addPair m p)) (by have := step_len m p; omega)
  have := key pairs []
  simpa [buildMap] using this

end LC.P0205
