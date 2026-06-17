import Lproofs.Schemes.Fold

/-! @lc 42 | name:Trapping Rain Water | scheme:fold | family:monotonic-stack | complexity:O(n) |
    source:https://leetcode.com/problems/trapping-rain-water/

    Water trapped above each column is `max 0 (min(leftWall, rightWall) − height)`, where the walls
    are the tallest bars to the left and right (both ≥ the column's own height). The total is the
    sum over columns. Correctness: given walls at least as tall as the column, the water *surface*
    sits exactly at the lower wall — `water + height = min(leftWall, rightWall)`. -/

namespace LC.P0042
open Interview.Patterns

/-- Water above one column, given its left/right bounding wall heights. -/
def waterAt (leftW rightW h : ℤ) : ℤ := max 0 (min leftW rightW - h)

/-- Total trapped water over all columns `(leftWall, rightWall, height)`. -/
def sol (cols : List (ℤ × ℤ × ℤ)) : ℤ :=
  (cols.map (fun c => waterAt c.1 c.2.1 c.2.2)).sum

/-- SCHEME (fold): the total is a streaming accumulation over the columns. -/
theorem cls : IsFold (fun cols : List (ℤ × ℤ × ℤ) =>
    cols.foldl (fun acc c => acc + waterAt c.1 c.2.1 c.2.2) 0) :=
  ⟨_, _, fun _ => rfl⟩

/-- CORRECT: with walls at least as tall as the column, the water surface reaches the lower wall
    (`water + height = min leftWall rightWall`), and the water is non-negative. -/
theorem corr (leftW rightW h : ℤ) (hL : h ≤ leftW) (hR : h ≤ rightW) :
    waterAt leftW rightW h + h = min leftW rightW ∧ 0 ≤ waterAt leftW rightW h := by
  unfold waterAt
  constructor <;> omega

end LC.P0042
