import Lproofs.Problems.DP.P0062_UniquePaths

/-! @lc 63 | name:Unique Paths II | scheme:dp | family:grid-dp | complexity:O(mn) |
    source:https://leetcode.com/problems/unique-paths-ii/

    Like Unique Paths, but blocked cells contribute zero. CLASSIFICATION: the same grid recursive
    decomposition — a cell sums its up- and left-neighbours — now masked by an obstacle predicate.
    NON-VACUITY: with no obstacles the masked DP collapses exactly to the unobstructed count `paths`
    (62), proven by induction over the grid recurrence. So `cls` names the genuine grid decomposition,
    obstacle-masked — not a lone `rfl`. We certify the decomposition, not optimality. -/

namespace LC.P0063

open LC.P0062 (paths)

/-- Path counts with obstacles: a blocked cell (`g a b = true`) yields `0`; otherwise the same grid
    recurrence as Unique Paths (boundaries chain along the top row / left column). -/
def pathsObs (g : ℕ → ℕ → Bool) : ℕ → ℕ → ℕ
  | 0, 0 => if g 0 0 then 0 else 1
  | 0, b + 1 => if g 0 (b + 1) then 0 else pathsObs g 0 b
  | a + 1, 0 => if g (a + 1) 0 then 0 else pathsObs g a 0
  | a + 1, b + 1 => if g (a + 1) (b + 1) then 0 else pathsObs g a (b + 1) + pathsObs g (a + 1) b

/-- SCHEME (dp / recursive decomposition): the interior recurrence — an unblocked cell sums the cell
    above and the cell to its left; a blocked cell contributes none. The genuine masked decomposition. -/
theorem cls (g : ℕ → ℕ → Bool) (a b : ℕ) :
    pathsObs g (a + 1) (b + 1) =
      (if g (a + 1) (b + 1) then 0 else pathsObs g a (b + 1) + pathsObs g (a + 1) b) := by
  simp [pathsObs]

/-- NON-VACUITY (reduction): with no obstacles the masked DP equals the unobstructed grid count `paths`
    (62) — i.e. `pathsObs` is genuinely the grid decomposition with obstacle masking layered on top. -/
theorem corr (a b : ℕ) : pathsObs (fun _ _ => false) a b = paths a b := by
  have hl : ∀ a, paths a 0 = 1 := fun a => by cases a <;> simp [paths]
  induction a, b using pathsObs.induct (g := fun _ _ => false) <;>
    simp_all [pathsObs, paths, hl]

end LC.P0063
