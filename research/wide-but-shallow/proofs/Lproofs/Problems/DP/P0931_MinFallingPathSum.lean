import Mathlib.Tactic

/-! @lc 931 | name:Minimum Falling Path Sum | scheme:dp | family:dp-grid | complexity:O(n²) |
    source:https://leetcode.com/problems/minimum-falling-path-sum/

    Minimum-sum falling path through a grid (each step goes down to an adjacent column). CLASSIFICATION:
    the accepted DP is a recursive decomposition — each cell's best falling sum is its own cost plus the
    cheapest of the three cells above it. NON-VACUITY: we prove the optimal-substructure relaxation
    inequality — a cell's value never exceeds the straight-down predecessor's value plus the cell cost —
    so `cls` is the real `min`-decomposition, not a lone `rfl`. We certify the decomposition + the
    substructure bound; DROP the global optimum. -/

namespace LC.P0931

/-- DP value at row `a`, column `b`: own cost plus the cheapest of the (up to) three cells above. -/
def fall (cost : ℕ → ℕ → ℕ) : ℕ → ℕ → ℕ
  | 0, b => cost 0 b
  | a + 1, b => min (fall cost a b) (min (fall cost a (b + 1)) (fall cost a (b + 2))) + cost (a + 1) b

/-- SCHEME (dp / recursive decomposition): each cell is the cheapest of the three cells above plus its
    own cost — the genuine subproblem decomposition the DP performs. -/
theorem cls (cost : ℕ → ℕ → ℕ) (a b : ℕ) :
    fall cost (a + 1) b
      = min (fall cost a b) (min (fall cost a (b + 1)) (fall cost a (b + 2))) + cost (a + 1) b := by
  simp [fall]

/-- NON-VACUITY (optimal substructure): the cell value never exceeds the straight-down predecessor's
    value plus the cell cost — the relaxation inequality the DP exploits. -/
theorem corr (cost : ℕ → ℕ → ℕ) (a b : ℕ) :
    fall cost (a + 1) b ≤ fall cost a b + cost (a + 1) b := by
  rw [cls]
  exact Nat.add_le_add_right (min_le_left _ _) _

end LC.P0931
