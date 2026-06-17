import Lproofs.Schemes.Bisect

/-! @lc 1011 | name:Capacity To Ship Packages Within D Days | scheme:bisection | family:binary-search |
    complexity:O(n log S) | source:https://leetcode.com/problems/capacity-to-ship-packages-within-d-days/ -/

namespace LC.P1011
open Interview.Patterns

/-- `feasible c` = the packages ship within the deadline using capacity `c`; monotone (more
    capacity never hurts). The editorial binary-searches the answer = least feasible capacity. -/
def sol (feasible : ℕ → Prop) [DecidablePred feasible] (h : ∃ c, feasible c) : ℕ := Nat.find h

/-- Spec: the answer is the least feasible capacity. -/
def spec (feasible : ℕ → Prop) (n : ℕ) : Prop := IsLeast {c | feasible c} n

/-- SCHEME (bisection): the monotone feasibility predicate is a threshold (`feasible n ↔ ans ≤ n`),
    the up-set structure that makes binary search on the answer correct. -/
theorem cls (feasible : ℕ → Prop) [DecidablePred feasible]
    (mono : ∀ a b, a ≤ b → feasible a → feasible b) (h : ∃ c, feasible c) (n : ℕ) :
    feasible n ↔ sol feasible h ≤ n :=
  bisection_threshold feasible mono h n

/-- CORRECT: the binary-search answer is the least feasible capacity. -/
theorem corr (feasible : ℕ → Prop) [DecidablePred feasible] (h : ∃ c, feasible c) :
    spec feasible (sol feasible h) :=
  bisection_isLeast feasible h

end LC.P1011
