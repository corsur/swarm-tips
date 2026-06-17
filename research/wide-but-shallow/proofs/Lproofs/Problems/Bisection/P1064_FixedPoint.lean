import Lproofs.Schemes.Bisect

/-! @lc 1064 | name:Fixed Point | scheme:bisection | family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/problems/fixed-point/

    In a sorted array of distinct integers, find the smallest index `i` with `arr[i] = i`. Since
    `arr[i] − i` is non-decreasing, `arr[i] ≥ i` is a monotone up-set, so a binary search locates the
    boundary. CLASSIFICATION: the predicate `reaches i` ("`i ≤ arr[i]`") is a monotone threshold, and
    the answer is its least satisfying index (the fixed point sits there when equality holds). `cls`
    certifies the threshold structure; `corr` that the answer is the least such index. -/

namespace LC.P1064
open Interview.Patterns

/-- `reaches i` — the value at `i` has caught up to the index (`i ≤ arr i`). -/
def reaches (arr : ℕ → ℤ) (i : ℕ) : Prop := (i : ℤ) ≤ arr i

instance (arr : ℕ → ℤ) : DecidablePred (reaches arr) :=
  fun i => inferInstanceAs (Decidable ((i : ℤ) ≤ arr i))

/-- The binary-search answer: the least index whose value has caught up to it. -/
def sol (arr : ℕ → ℤ) (h : ∃ i, reaches arr i) : ℕ := Nat.find h

/-- `reaches` is a monotone up-set when `arr i − i` is non-decreasing (modeled directly): once the
    value catches up, it stays caught up. -/
theorem reaches_mono (arr : ℕ → ℤ) (hmono : ∀ a b, a ≤ b → reaches arr a → reaches arr b) :
    ∀ a b, a ≤ b → reaches arr a → reaches arr b := hmono

/-- SCHEME (bisection): the predicate is a monotone threshold — `reaches i ↔ answer ≤ i`. -/
theorem cls (arr : ℕ → ℤ) (hmono : ∀ a b, a ≤ b → reaches arr a → reaches arr b)
    (h : ∃ i, reaches arr i) (n : ℕ) : reaches arr n ↔ sol arr h ≤ n :=
  bisection_threshold (reaches arr) (reaches_mono arr hmono) h n

/-- The answer is the least index whose value has caught up — the fixed-point boundary. -/
theorem corr (arr : ℕ → ℤ) (h : ∃ i, reaches arr i) :
    IsLeast {i | reaches arr i} (sol arr h) :=
  bisection_isLeast (reaches arr) h

end LC.P1064
