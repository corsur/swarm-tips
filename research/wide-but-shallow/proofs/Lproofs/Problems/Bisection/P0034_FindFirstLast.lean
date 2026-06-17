import Lproofs.Schemes.Bisect

/-! @lc 34 | name:Find First and Last Position of Element in Sorted Array | scheme:bisection |
    family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/problems/find-first-and-last-position-of-element-in-sorted-array/ -/

namespace LC.P0034
open Interview.Patterns

/-- `atLeast i` = the `i`-th element of the sorted array is `≥ target`; monotone in `i` (sortedness).
    The first position of `target` is the least such index — the binary-search boundary. -/
def sol (atLeast : ℕ → Prop) [DecidablePred atLeast] (h : ∃ i, atLeast i) : ℕ := Nat.find h

/-- Spec: the answer is the least index reaching the target value. -/
def spec (atLeast : ℕ → Prop) (n : ℕ) : Prop := IsLeast {i | atLeast i} n

/-- SCHEME (bisection): the monotone predicate is a threshold (`atLeast n ↔ ans ≤ n`). -/
theorem cls (atLeast : ℕ → Prop) [DecidablePred atLeast]
    (mono : ∀ a b, a ≤ b → atLeast a → atLeast b) (h : ∃ i, atLeast i) (n : ℕ) :
    atLeast n ↔ sol atLeast h ≤ n :=
  bisection_threshold atLeast mono h n

/-- CORRECT: the binary-search answer is the least qualifying index (the first position). -/
theorem corr (atLeast : ℕ → Prop) [DecidablePred atLeast] (h : ∃ i, atLeast i) :
    spec atLeast (sol atLeast h) :=
  bisection_isLeast atLeast h

end LC.P0034
