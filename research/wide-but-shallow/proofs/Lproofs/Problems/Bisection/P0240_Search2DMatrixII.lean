import Lproofs.Schemes.Bisect

/-! @lc 240 | name:Search a 2D Matrix II | scheme:bisection | family:binary-search |
    complexity:O(m log n) | source:https://leetcode.com/problems/search-a-2d-matrix-ii/

    Each row is sorted, so within a concrete row `row j` the predicate `atLeast row target j` =
    "the `j`-th entry reaches the target" is monotone in `j`; binary search locates the boundary and
    the target is present in that row iff the boundary entry equals it (repeated over the rows).
    CLASSIFICATION: bisection on each row's monotone threshold. `cls` certifies the threshold structure
    of the concrete row; `corr` that the answer is the least qualifying column. -/

namespace LC.P0240
open Interview.Patterns

/-- `atLeast row target j` — the `j`-th entry of the sorted `row` reaches `target`. -/
def atLeast (row : ℕ → ℕ) (target j : ℕ) : Prop := target ≤ row j

instance (row : ℕ → ℕ) (target : ℕ) : DecidablePred (atLeast row target) :=
  fun j => inferInstanceAs (Decidable (target ≤ row j))

/-- The binary-search answer: the least column whose entry reaches the target. -/
def sol (row : ℕ → ℕ) (target : ℕ) (h : ∃ j, atLeast row target j) : ℕ := Nat.find h

/-- On a sorted row `atLeast` is a monotone up-set: once reached, it stays. -/
theorem atLeast_mono (row : ℕ → ℕ) (hmono : Monotone row) (target : ℕ) :
    ∀ a b, a ≤ b → atLeast row target a → atLeast row target b :=
  fun a b hab ha => le_trans ha (hmono hab)

/-- SCHEME (bisection): the per-row predicate is a monotone threshold — `atLeast j ↔ answer ≤ j`. -/
theorem cls (row : ℕ → ℕ) (hmono : Monotone row) (target : ℕ) (h : ∃ j, atLeast row target j) (n : ℕ) :
    atLeast row target n ↔ sol row target h ≤ n :=
  bisection_threshold (atLeast row target) (atLeast_mono row hmono target) h n

/-- CORRECT: the binary-search answer is the least column of `row` whose entry reaches the target. -/
theorem corr (row : ℕ → ℕ) (target : ℕ) (h : ∃ j, atLeast row target j) :
    IsLeast {j | atLeast row target j} (sol row target h) :=
  bisection_isLeast (atLeast row target) h

end LC.P0240
