import Lproofs.Schemes.Bisect

/-! @lc 528 | name:Random Pick with Weight | scheme:bisection | family:binary-search |
    complexity:O(log n) | source:https://leetcode.com/problems/random-pick-with-weight/

    Pick an index with probability proportional to its weight: draw a target in `[0, total)` and binary
    search the prefix-sum array for the first prefix strictly above it. CLASSIFICATION: the predicate
    `above i` ("prefix sum `i` exceeds the target") is a monotone up-set (prefix sums increase), so the
    binary search returns its least satisfying index — the picked bucket. `cls` certifies the threshold
    structure; `corr` that the answer is the least such index. We certify the monotone-threshold
    structure the binary search exploits. -/

namespace LC.P0528
open Interview.Patterns

/-- `above i` — the `i`-th prefix sum is strictly above the drawn target (prefix sums are increasing). -/
def above (prefix' : ℕ → ℕ) (target i : ℕ) : Prop := target < prefix' i

instance (prefix' : ℕ → ℕ) (target : ℕ) : DecidablePred (above prefix' target) :=
  fun i => inferInstanceAs (Decidable (target < prefix' i))

/-- The picked bucket: the least index whose prefix sum exceeds the target. -/
def sol (prefix' : ℕ → ℕ) (target : ℕ) (h : ∃ i, above prefix' target i) : ℕ := Nat.find h

/-- `above` is a monotone up-set when prefix sums are monotone: once exceeded, it stays. -/
theorem above_mono (prefix' : ℕ → ℕ) (hmono : Monotone prefix') (target : ℕ) :
    ∀ a b, a ≤ b → above prefix' target a → above prefix' target b :=
  fun a b hab ha => lt_of_lt_of_le ha (hmono hab)

/-- SCHEME (bisection): the predicate is a monotone threshold — `above i ↔ bucket ≤ i`. -/
theorem cls (prefix' : ℕ → ℕ) (hmono : Monotone prefix') (target : ℕ)
    (h : ∃ i, above prefix' target i) (n : ℕ) :
    above prefix' target n ↔ sol prefix' target h ≤ n :=
  bisection_threshold (above prefix' target) (above_mono prefix' hmono target) h n

/-- The bucket is the least prefix-sum index above the target (the weighted pick). -/
theorem corr (prefix' : ℕ → ℕ) (target : ℕ) (h : ∃ i, above prefix' target i) :
    IsLeast {i | above prefix' target i} (sol prefix' target h) :=
  bisection_isLeast (above prefix' target) h

end LC.P0528
