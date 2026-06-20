import Lproofs.Schemes.Bisect

/-! @lc 1818 | name:Minimum Absolute Sum Difference | scheme:bisection | family:binary-search |
    complexity:O(n log n) | source:https://leetcode.com/problems/minimum-absolute-sum-difference/

    For each position the editorial binary-searches a concrete sorted array `nums1 j` for the value
    closest to a target, via the monotone predicate `atLeast nums1 target j` = "the `j`-th sorted value
    reaches `target`". The closest candidate sits at the boundary — the least such index.
    CLASSIFICATION: bisection on the monotone threshold of the concrete sorted array. `cls` certifies
    the threshold structure; `corr` that the answer is the least qualifying (insertion) index. -/

namespace LC.P1818
open Interview.Patterns

/-- `atLeast nums1 target j` — the `j`-th value of sorted `nums1` reaches `target`. -/
def atLeast (nums1 : ℕ → ℕ) (target j : ℕ) : Prop := target ≤ nums1 j

instance (nums1 : ℕ → ℕ) (target : ℕ) : DecidablePred (atLeast nums1 target) :=
  fun j => inferInstanceAs (Decidable (target ≤ nums1 j))

/-- The binary-search answer: the least index whose value reaches `target` (the insertion boundary). -/
def sol (nums1 : ℕ → ℕ) (target : ℕ) (h : ∃ j, atLeast nums1 target j) : ℕ := Nat.find h

/-- On the sorted array `atLeast` is a monotone up-set: once reached, it stays. -/
theorem atLeast_mono (nums1 : ℕ → ℕ) (hmono : Monotone nums1) (target : ℕ) :
    ∀ a b, a ≤ b → atLeast nums1 target a → atLeast nums1 target b :=
  fun a b hab ha => le_trans ha (hmono hab)

/-- SCHEME (bisection): the predicate is a monotone threshold — `atLeast j ↔ answer ≤ j`. -/
theorem cls (nums1 : ℕ → ℕ) (hmono : Monotone nums1) (target : ℕ) (h : ∃ j, atLeast nums1 target j)
    (n : ℕ) : atLeast nums1 target n ↔ sol nums1 target h ≤ n :=
  bisection_threshold (atLeast nums1 target) (atLeast_mono nums1 hmono target) h n

/-- CORRECT: the binary-search answer is the least index of `nums1` whose value reaches `target`. -/
theorem corr (nums1 : ℕ → ℕ) (target : ℕ) (h : ∃ j, atLeast nums1 target j) :
    IsLeast {j | atLeast nums1 target j} (sol nums1 target h) :=
  bisection_isLeast (atLeast nums1 target) h

end LC.P1818
