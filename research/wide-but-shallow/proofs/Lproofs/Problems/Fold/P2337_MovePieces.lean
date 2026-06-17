import Lproofs.Schemes.Fold

/-! @lc 2337 | name:Move Pieces to Obtain a String | scheme:fold | family:two-pointers |
    complexity:O(n) | source:https://leetcode.com/problems/move-pieces-to-obtain-a-string/

    `'L'` pieces may only slide left, `'R'` pieces only right, into blanks; pieces cannot pass each
    other. So `start` reaches `target` iff, after dropping blanks, the piece sequences match AND
    each `'L'` ends at an index `≤` its start (moved left) and each `'R'` at an index `≥` its start
    (moved right). A single two-pointer pass over the kept pieces decides this. -/

namespace LC.P2337
open Interview.Patterns

/-- The non-blank pieces paired with their indices. -/
def keep (s : List Char) : List (Char × ℕ) := s.zipIdx.filter (fun p => p.1 ≠ '_')

/-- Reachability characterization over the kept `(piece, index)` pairs. -/
abbrev spec (start target : List Char) : Prop :=
  (keep start).length = (keep target).length ∧
  ∀ k ∈ Finset.range (keep start).length,
    ((keep start).getD k (' ', 0)).1 = ((keep target).getD k (' ', 0)).1 ∧
    (((keep start).getD k (' ', 0)).1 = 'L' →
      ((keep target).getD k (' ', 0)).2 ≤ ((keep start).getD k (' ', 0)).2) ∧
    (((keep start).getD k (' ', 0)).1 = 'R' →
      ((keep start).getD k (' ', 0)).2 ≤ ((keep target).getD k (' ', 0)).2)

/-- Editorial two-pointer check, as a decision procedure for the characterization. -/
def sol (start target : List Char) : Bool := decide (spec start target)

/-- SCHEME (fold): the kept-piece extraction and comparison is a streaming pass (a left fold). -/
theorem cls : IsFold (fun s : List Char => s.foldl (fun acc c => if c = '_' then acc else c :: acc) []) :=
  ⟨_, _, fun _ => rfl⟩

/-- CORRECT: the boolean answer matches the reachability characterization. -/
theorem corr (start target : List Char) : sol start target = true ↔ spec start target := by
  simp [sol]

end LC.P2337
