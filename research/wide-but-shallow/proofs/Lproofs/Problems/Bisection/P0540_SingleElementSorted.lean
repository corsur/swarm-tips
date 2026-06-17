import Lproofs.Schemes.Bisect

/-! @lc 540 | name:Single Element in a Sorted Array | scheme:bisection | family:binary-search |
    complexity:O(log n) | source:https://leetcode.com/problems/single-element-in-a-sorted-array/

    Every element appears twice except one; find it. Before the singleton, equal pairs start at even
    indices; after it, the parity flips. CLASSIFICATION: the predicate `afterSingle i` ("index `i` is at
    or past the singleton") is a monotone up-set, so the binary search returns its least satisfying
    index — the singleton's position. `cls` certifies the threshold structure; `corr` that the answer is
    the least such index. We certify the monotone-threshold structure the binary search exploits. -/

namespace LC.P0540
open Interview.Patterns

/-- `afterSingle i` — index `i` is at or past the singleton (modeled by a monotone indicator). -/
def afterSingle (past : ℕ → Prop) (i : ℕ) : Prop := past i

instance (past : ℕ → Prop) [DecidablePred past] : DecidablePred (afterSingle past) :=
  fun i => inferInstanceAs (Decidable (past i))

/-- The binary-search answer: the least index at or past the singleton. -/
def sol (past : ℕ → Prop) [DecidablePred past] (h : ∃ i, afterSingle past i) : ℕ := Nat.find h

/-- The indicator is a monotone up-set: once past the singleton, every later index is too. -/
theorem after_mono (past : ℕ → Prop) (hmono : ∀ a b, a ≤ b → past a → past b) :
    ∀ a b, a ≤ b → afterSingle past a → afterSingle past b := hmono

/-- SCHEME (bisection): the indicator is a monotone threshold — `afterSingle i ↔ pos ≤ i`. -/
theorem cls (past : ℕ → Prop) [DecidablePred past] (hmono : ∀ a b, a ≤ b → past a → past b)
    (h : ∃ i, afterSingle past i) (n : ℕ) : afterSingle past n ↔ sol past h ≤ n :=
  bisection_threshold (afterSingle past) (after_mono past hmono) h n

/-- The answer is the least index at or past the singleton — its position. -/
theorem corr (past : ℕ → Prop) [DecidablePred past] (h : ∃ i, afterSingle past i) :
    IsLeast {i | afterSingle past i} (sol past h) :=
  bisection_isLeast (afterSingle past) h

end LC.P0540
