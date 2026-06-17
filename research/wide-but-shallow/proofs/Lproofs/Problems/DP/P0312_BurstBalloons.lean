import Lproofs.Schemes.Fold

/-! @lc 312 | name:Burst Balloons | scheme:dp | family:interval-dp | complexity:O(n³) |
    source:https://leetcode.com/problems/burst-balloons/

    Interval DP over the open interval `(i, j)`: choose the last balloon `k` to burst, splitting into the
    sub-intervals `(i, k)` and `(k, j)`. CLASSIFICATION: a recursive decomposition on the interval that
    recurses on both sides of the chosen split point `k` (`cls`). NON-VACUITY: we prove the genuine base
    case — an interval with no balloon strictly inside (`j ≤ i + 1`) yields `0` (`corr`) — pinning the
    DP to real interval work. We certify the interval split recurrence + the base; DROP the maximum
    coins value. -/

namespace LC.P0312

/-- Max coins over the open interval `(i, j)`; the fold ranges the last-burst balloon `k` (fuel-bounded). -/
def burst (nums : ℕ → ℕ) : ℕ → ℕ → ℕ → ℕ
  | 0, _, _ => 0
  | f + 1, i, j =>
      if j ≤ i + 1 then 0
      else (List.range j).foldl
        (fun acc k =>
          if i < k ∧ k < j then max acc (burst nums f i k + burst nums f k j + nums i * nums k * nums j)
          else acc) 0

/-- SCHEME (dp / interval recurrence): for a nonempty interval the value ranges over the last-burst
    balloon `k`, recursing on the two sub-intervals `(i, k)` and `(k, j)`. -/
theorem cls (nums : ℕ → ℕ) (f i j : ℕ) (hij : i + 1 < j) :
    burst nums (f + 1) i j =
      (List.range j).foldl
        (fun acc k =>
          if i < k ∧ k < j then max acc (burst nums f i k + burst nums f k j + nums i * nums k * nums j)
          else acc) 0 := by
  simp only [burst]
  rw [if_neg (by omega : ¬ (j ≤ i + 1))]

/-- NON-VACUITY (base): an interval with no balloon strictly inside yields zero coins. -/
theorem corr (nums : ℕ → ℕ) (f i j : ℕ) (h : j ≤ i + 1) : burst nums (f + 1) i j = 0 := by
  simp only [burst, if_pos h]

end LC.P0312
