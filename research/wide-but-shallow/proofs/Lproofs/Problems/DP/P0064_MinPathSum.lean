import Mathlib.Tactic

/-! @lc 64 | name:Minimum Path Sum | scheme:dp | family:grid-dp | complexity:O(mn) |
    source:https://leetcode.com/problems/minimum-path-sum/

    Minimum-cost monotone path (right/down) across a cost grid. CLASSIFICATION: the accepted DP is a
    genuine recursive decomposition — each cell's value is its own cost plus the cheaper of the cell
    above and the cell to the left, with additive borders. NON-VACUITY: we prove the optimal-substructure
    relaxation inequalities — the cell value never exceeds either predecessor's path — so `cls` is not a
    lone `rfl` but the real `min`-decomposition. We certify the decomposition + the substructure bound;
    DROP the global optimality-over-all-paths claim. -/

namespace LC.P0064

/-- DP value at the cell `a` rows down, `b` cols right: own cost plus the cheaper predecessor. -/
def minPath (cost : ℕ → ℕ → ℕ) : ℕ → ℕ → ℕ
  | 0, 0 => cost 0 0
  | 0, b + 1 => minPath cost 0 b + cost 0 (b + 1)
  | a + 1, 0 => minPath cost a 0 + cost (a + 1) 0
  | a + 1, b + 1 => min (minPath cost a (b + 1)) (minPath cost (a + 1) b) + cost (a + 1) (b + 1)

/-- SCHEME (dp / recursive decomposition): the interior recurrence — each cell is the cheaper of its
    up- and left-neighbours plus its own cost. The genuine subproblem decomposition the DP performs. -/
theorem cls (cost : ℕ → ℕ → ℕ) (a b : ℕ) :
    minPath cost (a + 1) (b + 1)
      = min (minPath cost a (b + 1)) (minPath cost (a + 1) b) + cost (a + 1) (b + 1) := by
  simp [minPath]

/-- NON-VACUITY (optimal substructure): the cell value never exceeds the up-neighbour's value plus the
    cell cost — the relaxation inequality the DP exploits. -/
theorem corr_up (cost : ℕ → ℕ → ℕ) (a b : ℕ) :
    minPath cost (a + 1) (b + 1) ≤ minPath cost a (b + 1) + cost (a + 1) (b + 1) := by
  rw [cls]
  exact Nat.add_le_add_right (min_le_left _ _) _

/-- NON-VACUITY (optimal substructure): symmetrically for the left-neighbour. -/
theorem corr_left (cost : ℕ → ℕ → ℕ) (a b : ℕ) :
    minPath cost (a + 1) (b + 1) ≤ minPath cost (a + 1) b + cost (a + 1) (b + 1) := by
  rw [cls]
  exact Nat.add_le_add_right (min_le_right _ _) _

end LC.P0064
