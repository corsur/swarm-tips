import Lproofs.Schemes.Bisect

/-! @lc 852 | name:Peak Index in a Mountain Array | scheme:bisection | family:binary-search |
    complexity:O(log n) | source:https://leetcode.com/problems/peak-index-in-a-mountain-array/ -/

namespace LC.P0852
open Interview.Patterns

/-- `peaked i` = index `i` is at or past the summit (`a i ≥ a (i+1)`); monotone in `i` for a
    mountain array. The peak is the least such index — the binary-search boundary. -/
def sol (peaked : ℕ → Prop) [DecidablePred peaked] (h : ∃ i, peaked i) : ℕ := Nat.find h

/-- Spec: the answer is the least index at/past the summit (the peak). -/
def spec (peaked : ℕ → Prop) (n : ℕ) : Prop := IsLeast {i | peaked i} n

/-- SCHEME (bisection): the monotone predicate is a threshold (`peaked n ↔ ans ≤ n`). -/
theorem cls (peaked : ℕ → Prop) [DecidablePred peaked]
    (mono : ∀ a b, a ≤ b → peaked a → peaked b) (h : ∃ i, peaked i) (n : ℕ) :
    peaked n ↔ sol peaked h ≤ n :=
  bisection_threshold peaked mono h n

/-- CORRECT: the binary-search answer is the least at/past-summit index (the peak). -/
theorem corr (peaked : ℕ → Prop) [DecidablePred peaked] (h : ∃ i, peaked i) :
    spec peaked (sol peaked h) :=
  bisection_isLeast peaked h

end LC.P0852
