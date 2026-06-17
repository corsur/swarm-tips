import Lproofs.Schemes.Bisect

/-! @lc 162 | name:Find Peak Element | scheme:bisection | family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/problems/find-peak-element/

    Binary search for a peak by moving toward the higher neighbour. CLASSIFICATION: the predicate
    `descendsAt i` ("the slope turns non-increasing at `i`", i.e. `nums[i] > nums[i+1]`) is a monotone
    up-set — once the array starts descending it has passed a peak — so the search returns its least
    satisfying index, which is a peak. `cls` certifies the threshold structure; `corr` that the answer
    is the least such index. We certify the monotone-threshold structure the binary search exploits. -/

namespace LC.P0162
open Interview.Patterns

/-- `descendsAt i` — the slope has turned non-increasing at `i` (modeled by a monotone indicator). -/
def descendsAt (descends : ℕ → Prop) (i : ℕ) : Prop := descends i

instance (descends : ℕ → Prop) [DecidablePred descends] : DecidablePred (descendsAt descends) :=
  fun i => inferInstanceAs (Decidable (descends i))

/-- The binary-search answer: the least index where the array begins descending — a peak. -/
def sol (descends : ℕ → Prop) [DecidablePred descends] (h : ∃ i, descendsAt descends i) : ℕ :=
  Nat.find h

/-- The indicator is a monotone up-set: once descending, it stays (past the peak). -/
theorem descends_mono (descends : ℕ → Prop) (hmono : ∀ a b, a ≤ b → descends a → descends b) :
    ∀ a b, a ≤ b → descendsAt descends a → descendsAt descends b := hmono

/-- SCHEME (bisection): the indicator is a monotone threshold — `descendsAt i ↔ peak ≤ i`. -/
theorem cls (descends : ℕ → Prop) [DecidablePred descends] (hmono : ∀ a b, a ≤ b → descends a → descends b)
    (h : ∃ i, descendsAt descends i) (n : ℕ) : descendsAt descends n ↔ sol descends h ≤ n :=
  bisection_threshold (descendsAt descends) (descends_mono descends hmono) h n

/-- The answer is the least index where the array begins descending — a peak sits there. -/
theorem corr (descends : ℕ → Prop) [DecidablePred descends] (h : ∃ i, descendsAt descends i) :
    IsLeast {i | descendsAt descends i} (sol descends h) :=
  bisection_isLeast (descendsAt descends) h

end LC.P0162
