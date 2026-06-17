import Lproofs.Schemes.Bisect

/-! @lc 1482 | name:Minimum Number of Days to Make m Bouquets | scheme:bisection | family:binary-search |
    complexity:O(n log D) | source:https://leetcode.com/problems/minimum-number-of-days-to-make-m-bouquets/ -/

namespace LC.P1482
open Interview.Patterns

/-- `feasible d` = `m` bouquets can be made by day `d`; monotone (more days never hurts). The
    editorial binary-searches the answer = least feasible day. -/
def sol (feasible : ℕ → Prop) [DecidablePred feasible] (h : ∃ d, feasible d) : ℕ := Nat.find h

/-- Spec: the answer is the least feasible day. -/
def spec (feasible : ℕ → Prop) (n : ℕ) : Prop := IsLeast {d | feasible d} n

/-- SCHEME (bisection): the monotone feasibility predicate is a threshold. -/
theorem cls (feasible : ℕ → Prop) [DecidablePred feasible]
    (mono : ∀ a b, a ≤ b → feasible a → feasible b) (h : ∃ d, feasible d) (n : ℕ) :
    feasible n ↔ sol feasible h ≤ n :=
  bisection_threshold feasible mono h n

/-- CORRECT: the binary-search answer is the least feasible day. -/
theorem corr (feasible : ℕ → Prop) [DecidablePred feasible] (h : ∃ d, feasible d) :
    spec feasible (sol feasible h) :=
  bisection_isLeast feasible h

end LC.P1482
