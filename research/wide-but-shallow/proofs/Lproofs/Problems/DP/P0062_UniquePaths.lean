import Mathlib.Data.Nat.Choose.Basic
import Mathlib.Tactic

/-! @lc 62 | name:Unique Paths | scheme:dp | family:grid-dp | complexity:O(mn) |
    source:https://leetcode.com/problems/unique-paths/

    Count monotone lattice paths across an m×n grid (moves right/down). CLASSIFICATION: the accepted
    DP is a genuine recursive decomposition over the grid — each cell's count is the sum of the cell
    above and the cell to the left, with a `1` boundary. NON-VACUITY: we prove the decomposition
    computes the real count via its closed form `C(a+b, a)` (Pascal's rule), so `cls` is not a lone
    `rfl`. We certify the decomposition structure + the count, not any optimality. -/

namespace LC.P0062

/-- Number of monotone paths reaching the cell `a` rows down and `b` cols right of the start — the
    grid DP's recursive decomposition: a cell sums its up- and left-neighbours; the borders are `1`. -/
def paths : ℕ → ℕ → ℕ
  | 0, _ => 1
  | _ + 1, 0 => 1
  | a + 1, b + 1 => paths a (b + 1) + paths (a + 1) b

/-- SCHEME (dp / recursive decomposition): the interior recurrence — each cell is the sum of the
    cell above and the cell to its left. This is the genuine subproblem decomposition the DP performs. -/
theorem cls (a b : ℕ) : paths (a + 1) (b + 1) = paths a (b + 1) + paths (a + 1) b := by
  simp [paths]

/-- NON-VACUITY (closed form): the decomposition computes the real path count `C(a+b, a)`, by Pascal's
    rule over the recurrence. This pins the DP to genuine counting work, not a re-encoding. -/
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

end LC.P0062
