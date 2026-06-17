import Lproofs.Schemes.Bisect

/-! @lc 1818 | name:Minimum Absolute Sum Difference | scheme:bisection | family:binary-search |
    complexity:O(n log n) | source:https://leetcode.com/problems/minimum-absolute-sum-difference/ -/

namespace LC.P1818
open Interview.Patterns

/-- For each position the editorial binary-searches sorted `nums1` for the value closest to the
    target, via the monotone predicate `atLeast i` = "the i-th sorted value is ≥ target". The
    closest candidate sits at the boundary — the least such index. -/
def sol (atLeast : ℕ → Prop) [DecidablePred atLeast] (h : ∃ i, atLeast i) : ℕ := Nat.find h

/-- Spec: the answer is the least index reaching the target (the insertion boundary). -/
def spec (atLeast : ℕ → Prop) (n : ℕ) : Prop := IsLeast {i | atLeast i} n

/-- SCHEME (bisection): the monotone predicate is a threshold. -/
theorem cls (atLeast : ℕ → Prop) [DecidablePred atLeast]
    (mono : ∀ a b, a ≤ b → atLeast a → atLeast b) (h : ∃ i, atLeast i) (n : ℕ) :
    atLeast n ↔ sol atLeast h ≤ n :=
  bisection_threshold atLeast mono h n

/-- CORRECT: the binary-search answer is the least qualifying index. -/
theorem corr (atLeast : ℕ → Prop) [DecidablePred atLeast] (h : ∃ i, atLeast i) :
    spec atLeast (sol atLeast h) :=
  bisection_isLeast atLeast h

end LC.P1818
