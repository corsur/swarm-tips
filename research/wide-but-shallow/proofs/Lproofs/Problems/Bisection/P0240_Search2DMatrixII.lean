import Lproofs.Schemes.Bisect

/-! @lc 240 | name:Search a 2D Matrix II | scheme:bisection | family:binary-search |
    complexity:O(m log n) | source:https://leetcode.com/problems/search-a-2d-matrix-ii/

    Each row is sorted, so within a row `atLeast j` = "the `j`-th entry is `≥ target`" is monotone
    in `j`; binary search locates the boundary and the target is present in that row iff the
    boundary entry equals it (repeated over the `m` rows). The scheme is bisection on each row's
    monotone threshold predicate. -/

namespace LC.P0240
open Interview.Patterns

/-- `atLeast j` = the `j`-th entry of a sorted row is `≥ target`, monotone in `j`. -/
def sol (atLeast : ℕ → Prop) [DecidablePred atLeast] (h : ∃ j, atLeast j) : ℕ := Nat.find h

/-- Spec: the answer is the least column index whose entry reaches the target. -/
def spec (atLeast : ℕ → Prop) (n : ℕ) : Prop := IsLeast {j | atLeast j} n

/-- SCHEME (bisection): the monotone per-row predicate is a threshold. -/
theorem cls (atLeast : ℕ → Prop) [DecidablePred atLeast]
    (mono : ∀ a b, a ≤ b → atLeast a → atLeast b) (h : ∃ j, atLeast j) (n : ℕ) :
    atLeast n ↔ sol atLeast h ≤ n :=
  bisection_threshold atLeast mono h n

/-- CORRECT: the binary-search answer is the least qualifying column index. -/
theorem corr (atLeast : ℕ → Prop) [DecidablePred atLeast] (h : ∃ j, atLeast j) :
    spec atLeast (sol atLeast h) :=
  bisection_isLeast atLeast h

end LC.P0240
