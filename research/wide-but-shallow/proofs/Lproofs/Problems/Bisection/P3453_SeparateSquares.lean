import Lproofs.Schemes.Bisect

/-! @lc 3453 | name:Separate Squares I | scheme:bisection | family:binary-search | complexity:O(n log) |
    source:https://leetcode.com/problems/separate-squares-i/ -/

namespace LC.P3453
open Interview.Patterns

/-- `enough y` = a horizontal line at height `y` leaves at least half the total area below it;
    monotone in `y` (raising the line never decreases the area below). The editorial binary-searches
    the least such height (the answer, to tolerance). -/
def sol (enough : ℕ → Prop) [DecidablePred enough] (h : ∃ y, enough y) : ℕ := Nat.find h

/-- Spec: the answer is the least height with at least half the area below. -/
def spec (enough : ℕ → Prop) (n : ℕ) : Prop := IsLeast {y | enough y} n

/-- SCHEME (bisection): the monotone predicate is a threshold. -/
theorem cls (enough : ℕ → Prop) [DecidablePred enough]
    (mono : ∀ a b, a ≤ b → enough a → enough b) (h : ∃ y, enough y) (n : ℕ) :
    enough n ↔ sol enough h ≤ n :=
  bisection_threshold enough mono h n

/-- CORRECT: the binary-search answer is the least qualifying height. -/
theorem corr (enough : ℕ → Prop) [DecidablePred enough] (h : ∃ y, enough y) :
    spec enough (sol enough h) :=
  bisection_isLeast enough h

end LC.P3453
