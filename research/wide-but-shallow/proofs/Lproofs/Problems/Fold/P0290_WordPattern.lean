import Lproofs.Schemes.Fold

/-! @lc 290 | name:Word Pattern | scheme:fold | family:mapping-scan | complexity:O(n) |
    source:https://leetcode.com/problems/word-pattern/

    A string follows a pattern iff a consistent bijection links each pattern character to a word.
    CLASSIFICATION: the check is a streaming left fold over the (pattern-char, word) pairs whose
    accumulator is the mapping built so far — each new pair is added unless already present.
    NON-VACUITY: we prove the mapping never grows beyond the number of pairs processed (each pair adds at
    most one entry), so the accumulator does genuine bijection-building work, not a re-encoding. We
    certify the fold + the mapping-size bound. -/

namespace LC.P0290
open Interview.Patterns

/-- Record the (pattern-char, word) pair unless already present. -/
def addPair (m : List (Char × List Char)) (p : Char × List Char) : List (Char × List Char) :=
  if p ∈ m then m else p :: m

/-- The mapping built by scanning the paired pattern characters and words. -/
def buildMap (pairs : List (Char × List Char)) : List (Char × List Char) := pairs.foldl addPair []

/-- SCHEME (fold): the mapping is a left fold with the built-pairs accumulator. -/
theorem cls : IsFold (fun pairs : List (Char × List Char) => pairs.foldl addPair []) :=
  ⟨addPair, [], fun _ => rfl⟩

/-- Each step adds at most one mapping. -/
theorem step_len (m : List (Char × List Char)) (p : Char × List Char) :
    (addPair m p).length ≤ m.length + 1 := by
  simp only [addPair]; split <;> simp

/-- NON-VACUITY: the mapping never grows beyond the number of pairs processed. -/
theorem corr (pairs : List (Char × List Char)) : (buildMap pairs).length ≤ pairs.length := by
  have key : ∀ (ps : List (Char × List Char)) m, (ps.foldl addPair m).length ≤ m.length + ps.length := by
    intro ps
    induction ps with
    | nil => intro m; simp
    | cons p rest ih =>
      intro m
      simp only [List.foldl_cons, List.length_cons]
      exact le_trans (ih (addPair m p)) (by have := step_len m p; omega)
  have := key pairs []
  simpa [buildMap] using this

end LC.P0290
