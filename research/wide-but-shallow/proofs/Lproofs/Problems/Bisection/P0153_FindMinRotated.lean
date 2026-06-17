import Lproofs.Schemes.Bisect

/-! @lc 153 | name:Find Minimum in Rotated Sorted Array | scheme:bisection | family:binary-search |
    complexity:O(log n) | source:https://leetcode.com/problems/find-minimum-in-rotated-sorted-array/

    A sorted array rotated at an unknown pivot; the minimum sits at the pivot. CLASSIFICATION: the
    predicate `atOrPastPivot i` ("index `i` is at or past the rotation point") is a monotone up-set, so
    the accepted binary search returns its least satisfying index — the pivot, where the minimum lives.
    `cls` certifies the threshold structure (`atOrPastPivot i ↔ pivot ≤ i`); `corr` that the pivot is
    the least such index. We certify the monotone-threshold structure the binary search exploits. -/

namespace LC.P0153
open Interview.Patterns

/-- `i` is at or past the pivot — equivalently the suffix from `i` is the lower (post-rotation) run.
    Modeled by a monotone indicator `pastPivot` (false on the high run, true from the pivot on). -/
def atOrPastPivot (pastPivot : ℕ → Prop) (i : ℕ) : Prop := pastPivot i

instance (pastPivot : ℕ → Prop) [DecidablePred pastPivot] :
    DecidablePred (atOrPastPivot pastPivot) := fun i => inferInstanceAs (Decidable (pastPivot i))

/-- The accepted binary-search answer: the least index at or past the pivot (where the minimum is). -/
def sol (pastPivot : ℕ → Prop) [DecidablePred pastPivot] (h : ∃ i, atOrPastPivot pastPivot i) : ℕ :=
  Nat.find h

/-- The indicator is a monotone up-set: once past the pivot, every later index is too. -/
theorem pivot_mono (pastPivot : ℕ → Prop) (hmono : ∀ a b, a ≤ b → pastPivot a → pastPivot b) :
    ∀ a b, a ≤ b → atOrPastPivot pastPivot a → atOrPastPivot pastPivot b := hmono

/-- SCHEME (bisection): the pivot indicator is a monotone threshold —
    `atOrPastPivot i ↔ pivot ≤ i` — exactly what the binary search exploits. -/
theorem cls (pastPivot : ℕ → Prop) [DecidablePred pastPivot]
    (hmono : ∀ a b, a ≤ b → pastPivot a → pastPivot b) (h : ∃ i, atOrPastPivot pastPivot i) (n : ℕ) :
    atOrPastPivot pastPivot n ↔ sol pastPivot h ≤ n :=
  bisection_threshold (atOrPastPivot pastPivot) (pivot_mono pastPivot hmono) h n

/-- The answer is the least index at or past the pivot — the location of the minimum. -/
theorem corr (pastPivot : ℕ → Prop) [DecidablePred pastPivot] (h : ∃ i, atOrPastPivot pastPivot i) :
    IsLeast {i | atOrPastPivot pastPivot i} (sol pastPivot h) :=
  bisection_isLeast (atOrPastPivot pastPivot) h

end LC.P0153
