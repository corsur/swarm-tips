import Mathlib.Data.Nat.Choose.Basic
import Mathlib.Tactic

/-! @lc 2912 | name:Number of Ways to Reach Destination in the Grid | scheme:dp | family:dp-grid |
    complexity:O(mn) | source:https://leetcode.com/

    Count lattice paths across a grid (moves right/down). CLASSIFICATION: the accepted DP is a recursive
    decomposition — each cell's count is the sum of the cell above and the cell to the left, with a `1`
    boundary. NON-VACUITY: we prove the decomposition computes the real count via its closed form
    `C(a+b, a)` (Pascal's rule), so `cls` is not a lone `rfl`. We certify the decomposition + the count. -/

namespace LC.P2912

/-- Number of monotone paths to the cell `a` down, `b` right of the start. -/
def paths : ℕ → ℕ → ℕ
  | 0, _ => 1
  | _ + 1, 0 => 1
  | a + 1, b + 1 => paths a (b + 1) + paths (a + 1) b

/-- SCHEME (dp / recursive decomposition): each interior cell is the sum of the cell above and the cell
    to its left — the genuine subproblem decomposition. -/
theorem cls (a b : ℕ) : paths (a + 1) (b + 1) = paths a (b + 1) + paths (a + 1) b := by
  simp [paths]

/-- NON-VACUITY (closed form): the decomposition computes the real path count `C(a+b, a)`. -/
theorem corr (a b : ℕ) : paths a b = (a + b).choose a := by
  induction a, b using paths.induct with
  | case1 b => simp [paths]
  | case2 a => simp [paths, Nat.choose_self]
  | case3 a b iha ihb =>
    rw [cls, iha, ihb]
    have h1 : a + (b + 1) = a + b + 1 := by ring
    have h2 : a + 1 + b = a + b + 1 := by ring
    have h3 : a + 1 + (b + 1) = a + b + 1 + 1 := by ring
    rw [h1, h2, h3, Nat.choose_succ_succ (a + b + 1) a]

end LC.P2912
