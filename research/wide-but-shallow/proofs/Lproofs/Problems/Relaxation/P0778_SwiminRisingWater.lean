import Lproofs.Schemes.Bisect

/-! @lc 778 | name:Swim in Rising Water | scheme:bisection | family:binary-search | complexity:O(V log V) |
    source:https://leetcode.com/problems/swim-in-rising-water/

    The least time to reach the destination is the least water level `t` at which a path exists using
    only cells of elevation `≤ t` (a min-max / bottleneck path). "A path exists by time `t`" is
    monotone in `t` — once swimmable, it stays swimmable — so the answer is a threshold, found by
    binary search: the least feasible time. -/

namespace LC.P0778
open Interview.Patterns

/-- `feasible t` = the destination is reachable using only cells with elevation `≤ t`; monotone in
    `t`. The editorial binary-searches the answer = least feasible time. -/
def sol (feasible : ℕ → Prop) [DecidablePred feasible] (h : ∃ t, feasible t) : ℕ := Nat.find h

/-- Spec: the answer is the least feasible time. -/
def spec (feasible : ℕ → Prop) (n : ℕ) : Prop := IsLeast {t | feasible t} n

/-- SCHEME (bisection): the monotone reachability predicate is a threshold. -/
theorem cls (feasible : ℕ → Prop) [DecidablePred feasible]
    (mono : ∀ a b, a ≤ b → feasible a → feasible b) (h : ∃ t, feasible t) (n : ℕ) :
    feasible n ↔ sol feasible h ≤ n :=
  bisection_threshold feasible mono h n

/-- CORRECT: the binary-search answer is the least feasible time. -/
theorem corr (feasible : ℕ → Prop) [DecidablePred feasible] (h : ∃ t, feasible t) :
    spec feasible (sol feasible h) :=
  bisection_isLeast feasible h

end LC.P0778
