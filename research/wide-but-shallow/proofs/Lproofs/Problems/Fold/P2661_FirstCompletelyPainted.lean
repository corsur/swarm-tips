import Lproofs.Schemes.Fold

/-! @lc 2661 | name:First Completely Painted Row or Column | scheme:bisection | family:binary-search |
    complexity:O(n) | source:https://leetcode.com/problems/first-completely-painted-row-or-column/

    Paint cells in arrival order; the answer is the LEAST step at which some row or column is fully
    painted. "Completed by step k" is monotone (painted cells stay painted), so it is a threshold —
    the answer is `Nat.find` of the completion predicate, characterized as the least such step. -/

namespace LC.P2661
open Interview.Patterns

/-- `complete k` = after the first `k` paints, some row or column is fully painted. -/
def sol (complete : ℕ → Prop) [DecidablePred complete] (h : ∃ k, complete k) : ℕ := Nat.find h

/-- Spec: the answer is the least step at which a line completes. -/
def spec (complete : ℕ → Prop) (n : ℕ) : Prop := IsLeast {k | complete k} n

/-- SCHEME (bisection): the monotone completion predicate is a threshold — `complete n` holds iff
    the least completing step `sol` is `≤ n`. This characterizes the actual answer `sol`. -/
theorem cls (complete : ℕ → Prop) [DecidablePred complete]
    (mono : ∀ a b, a ≤ b → complete a → complete b) (h : ∃ k, complete k) (n : ℕ) :
    complete n ↔ sol complete h ≤ n :=
  bisection_threshold complete mono h n

/-- CORRECT: the answer is the least completing step. -/
theorem corr (complete : ℕ → Prop) [DecidablePred complete] (h : ∃ k, complete k) :
    spec complete (sol complete h) :=
  ⟨Nat.find_spec h, fun _ hk => Nat.find_min' h hk⟩

end LC.P2661
