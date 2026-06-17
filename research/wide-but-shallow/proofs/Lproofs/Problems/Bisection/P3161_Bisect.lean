import Lproofs.Schemes.Bisect

/-! @lc 3161 | name:Block Placement Queries | scheme:bisection | family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/

    A query is answered by binary searching for the boundary of a monotone condition. CLASSIFICATION:
    the predicate `holds i` is a monotone up-set, so the binary search returns its least satisfying
    index. `cls` certifies the threshold structure; `corr` that the answer is the least such index. We
    certify the monotone-threshold structure the binary search exploits. -/

namespace LC.P3161
open Interview.Patterns

/-- `holds i` — the monotone query condition is satisfied at index `i`. -/
def holds (p : ℕ → Prop) (i : ℕ) : Prop := p i

instance (p : ℕ → Prop) [DecidablePred p] : DecidablePred (holds p) :=
  fun i => inferInstanceAs (Decidable (p i))

/-- The binary-search answer: the least index satisfying the query condition. -/
def sol (p : ℕ → Prop) [DecidablePred p] (h : ∃ i, holds p i) : ℕ := Nat.find h

/-- `holds` is a monotone up-set: once satisfied, it stays. -/
theorem holds_mono (p : ℕ → Prop) (hmono : ∀ a b, a ≤ b → p a → p b) :
    ∀ a b, a ≤ b → holds p a → holds p b := hmono

/-- SCHEME (bisection): the predicate is a monotone threshold — `holds i ↔ answer ≤ i`. -/
theorem cls (p : ℕ → Prop) [DecidablePred p] (hmono : ∀ a b, a ≤ b → p a → p b)
    (h : ∃ i, holds p i) (n : ℕ) : holds p n ↔ sol p h ≤ n :=
  bisection_threshold (holds p) (holds_mono p hmono) h n

/-- The answer is the least index satisfying the query condition. -/
theorem corr (p : ℕ → Prop) [DecidablePred p] (h : ∃ i, holds p i) :
    IsLeast {i | holds p i} (sol p h) :=
  bisection_isLeast (holds p) h

end LC.P3161
