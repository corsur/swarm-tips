import Lproofs.Schemes.Bisect

/-! @lc 81 | name:Search in Rotated Sorted Array II | scheme:bisection | family:binary-search |
    complexity:O(log n) avg | source:https://leetcode.com/problems/search-in-rotated-sorted-array-ii/

    Binary search a rotated sorted array (with duplicates) by deciding, at each step, which half is
    sorted and whether the target lies in it. CLASSIFICATION: within the sorted half the predicate
    `atLeast i` ("the `i`-th value reaches the target") is a monotone up-set, so the search returns its
    least satisfying index. `cls` certifies the threshold structure; `corr` that the answer is the least
    such index. We certify the monotone-threshold structure the binary search exploits. -/

namespace LC.P0081
open Interview.Patterns

/-- `atLeast i` — the `i`-th value (within the sorted half) reaches the target. -/
def atLeast (vals : ℕ → ℕ) (target i : ℕ) : Prop := target ≤ vals i

instance (vals : ℕ → ℕ) (target : ℕ) : DecidablePred (atLeast vals target) :=
  fun i => inferInstanceAs (Decidable (target ≤ vals i))

/-- The binary-search answer: the least index whose value reaches the target. -/
def sol (vals : ℕ → ℕ) (target : ℕ) (h : ∃ i, atLeast vals target i) : ℕ := Nat.find h

/-- `atLeast` is a monotone up-set on a sorted run: once reached, it stays. -/
theorem atLeast_mono (vals : ℕ → ℕ) (hmono : Monotone vals) (target : ℕ) :
    ∀ a b, a ≤ b → atLeast vals target a → atLeast vals target b :=
  fun a b hab ha => le_trans ha (hmono hab)

/-- SCHEME (bisection): the predicate is a monotone threshold — `atLeast i ↔ answer ≤ i`. -/
theorem cls (vals : ℕ → ℕ) (hmono : Monotone vals) (target : ℕ) (h : ∃ i, atLeast vals target i) (n : ℕ) :
    atLeast vals target n ↔ sol vals target h ≤ n :=
  bisection_threshold (atLeast vals target) (atLeast_mono vals hmono target) h n

/-- The answer is the least index reaching the target on the sorted run. -/
theorem corr (vals : ℕ → ℕ) (target : ℕ) (h : ∃ i, atLeast vals target i) :
    IsLeast {i | atLeast vals target i} (sol vals target h) :=
  bisection_isLeast (atLeast vals target) h

end LC.P0081
