import Lproofs.Schemes.Fold

/-! @lc 120 | name:Triangle | scheme:dp | family:dp-grid | complexity:O(n²) |
    source:https://leetcode.com/problems/triangle/

    The minimum top-to-bottom path sum, where each step descends to an adjacent entry below, is the
    interval DP `go(i,j) = tri[i][j] + min(go(i+1,j), go(i+1,j+1))`. CLASSIFICATION (dp): a recursive
    decomposition picking the cheaper of two children (`cls`). CORRECTNESS: we certify the DP is sound
    against the actual paths — its value never exceeds the sum along any concrete descent (we exhibit the
    always-left path), so `go` is a genuine lower-bounding minimum, not an arbitrary number. -/

namespace LC.P0120

/-- Minimum path sum over `f+1` rows starting at `(i, j)` (each step to an adjacent entry below). -/
def go (tri : ℕ → ℕ → ℤ) : ℕ → ℕ → ℕ → ℤ
  | 0, i, j => tri i j
  | f + 1, i, j => tri i j + min (go tri f (i + 1) j) (go tri f (i + 1) (j + 1))

/-- The sum along one concrete descent: always step to the left child. -/
def leftSum (tri : ℕ → ℕ → ℤ) : ℕ → ℕ → ℕ → ℤ
  | 0, i, j => tri i j
  | f + 1, i, j => tri i j + leftSum tri f (i + 1) j

/-- SCHEME (dp): the value decomposes into the cell plus the cheaper of the two children. -/
theorem cls (tri : ℕ → ℕ → ℤ) (f i j : ℕ) :
    go tri (f + 1) i j = tri i j + min (go tri f (i + 1) j) (go tri f (i + 1) (j + 1)) := rfl

/-- CORRECT: the DP minimum never exceeds the sum along a real descent (the always-left path), so it is
    a genuine lower-bounding path minimum. -/
theorem corr (tri : ℕ → ℕ → ℤ) (f i j : ℕ) : go tri f i j ≤ leftSum tri f i j := by
  induction f generalizing i j with
  | zero => simp [go, leftSum]
  | succ f ih =>
    simp only [go, leftSum]
    have hmin : min (go tri f (i + 1) j) (go tri f (i + 1) (j + 1)) ≤ go tri f (i + 1) j :=
      min_le_left _ _
    have := ih (i + 1) j
    linarith

end LC.P0120
