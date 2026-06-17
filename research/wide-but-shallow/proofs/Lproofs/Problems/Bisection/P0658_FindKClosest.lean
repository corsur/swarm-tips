import Lproofs.Schemes.Bisect

/-! @lc 658 | name:Find K Closest Elements | scheme:bisection | family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/problems/find-k-closest-elements/

    Return the `k` elements closest to `x` from a sorted array; binary search the left boundary of the
    size-`k` window. CLASSIFICATION: the predicate `windowFits i` ("starting the window at `i` is at
    least as good as starting at `i−1`") is a monotone up-set over candidate starts, so the binary
    search returns its least satisfying index — the optimal left boundary. `cls` certifies the threshold
    structure; `corr` that the answer is the least such start. We certify the monotone-threshold
    structure the binary search exploits. -/

namespace LC.P0658
open Interview.Patterns

/-- `windowFits i` — starting the closest-`k` window at index `i` is preferred (monotone in `i`). -/
def windowFits (fits : ℕ → Prop) (i : ℕ) : Prop := fits i

instance (fits : ℕ → Prop) [DecidablePred fits] : DecidablePred (windowFits fits) :=
  fun i => inferInstanceAs (Decidable (fits i))

/-- The binary-search answer: the least window start that fits. -/
def sol (fits : ℕ → Prop) [DecidablePred fits] (h : ∃ i, windowFits fits i) : ℕ := Nat.find h

/-- The predicate is a monotone up-set: once a start is preferred, every later start is too. -/
theorem fits_mono (fits : ℕ → Prop) (hmono : ∀ a b, a ≤ b → fits a → fits b) :
    ∀ a b, a ≤ b → windowFits fits a → windowFits fits b := hmono

/-- SCHEME (bisection): the predicate is a monotone threshold — `windowFits i ↔ start ≤ i`. -/
theorem cls (fits : ℕ → Prop) [DecidablePred fits] (hmono : ∀ a b, a ≤ b → fits a → fits b)
    (h : ∃ i, windowFits fits i) (n : ℕ) : windowFits fits n ↔ sol fits h ≤ n :=
  bisection_threshold (windowFits fits) (fits_mono fits hmono) h n

/-- The answer is the least fitting window start — the closest-`k` left boundary. -/
theorem corr (fits : ℕ → Prop) [DecidablePred fits] (h : ∃ i, windowFits fits i) :
    IsLeast {i | windowFits fits i} (sol fits h) :=
  bisection_isLeast (windowFits fits) h

end LC.P0658
