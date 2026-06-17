import Lproofs.Classify

/-! @lc 70 | name:Climbing Stairs | scheme:dp | family:dp-linear | complexity:O(n) |
    source:https://leetcode.com/problems/climbing-stairs/ -/

namespace LC.P0070
open Interview.Classify

/-- Spec: number of ways to climb `n` stairs taking 1 or 2 at a time (the DP recurrence). -/
def spec (n : ℕ) : ℕ := stairs n

/-- Editorial O(1)-space iterative DP: carry the last two values. -/
def sol (n : ℕ) : ℕ := (stairsIter n).1

/-- SCHEME: the iterative state computes the DP fold (catamorphism) — the carried pair is
    exactly `(stairs n, stairs (n+1))`. -/
theorem cls (n : ℕ) : stairsIter n = (stairs n, stairs (n + 1)) := stairsIter_eq n

/-- CORRECT: the iterative solution equals the recurrence (the spec). -/
theorem corr (n : ℕ) : sol n = spec n := classify_climbingStairs n

end LC.P0070
