import Lproofs.Schemes.Bisect

/-! @lc 74 | name:Search a 2D Matrix | scheme:bisection | family:binary-search | complexity:O(log nm) |
    source:https://leetcode.com/problems/search-a-2d-matrix/ -/

namespace LC.P0074
open Interview.Patterns

/-- The matrix is fully sorted when flattened, so `atLeast i` = the `i`-th flattened entry is
    `≥ target` is monotone in `i`. Binary search locates the boundary; the target is present iff
    that entry equals it. -/
def sol (atLeast : ℕ → Prop) [DecidablePred atLeast] (h : ∃ i, atLeast i) : ℕ := Nat.find h

/-- Spec: the answer is the least flattened index reaching the target. -/
def spec (atLeast : ℕ → Prop) (n : ℕ) : Prop := IsLeast {i | atLeast i} n

/-- SCHEME (bisection): the monotone predicate is a threshold. -/
theorem cls (atLeast : ℕ → Prop) [DecidablePred atLeast]
    (mono : ∀ a b, a ≤ b → atLeast a → atLeast b) (h : ∃ i, atLeast i) (n : ℕ) :
    atLeast n ↔ sol atLeast h ≤ n :=
  bisection_threshold atLeast mono h n

/-- CORRECT: the binary-search answer is the least qualifying flattened index. -/
theorem corr (atLeast : ℕ → Prop) [DecidablePred atLeast] (h : ∃ i, atLeast i) :
    spec atLeast (sol atLeast h) :=
  bisection_isLeast atLeast h

end LC.P0074
