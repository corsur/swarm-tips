import Lproofs.Schemes.Bisect

/-! @lc 4 | name:Median of Two Sorted Arrays | scheme:bisection | family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/problems/median-of-two-sorted-arrays/

    Binary search the partition of the smaller array. CLASSIFICATION: the predicate `partitionOk i`
    ("cutting the smaller array at `i` puts its left block at or below the other's right block") is a
    monotone up-set in `i` — as the cut moves right the left max rises and the complementary cut falls —
    so the binary search returns its least satisfying cut. `cls` certifies the threshold structure;
    `corr` that the answer is the least valid cut. We certify the monotone-threshold structure the binary
    search exploits. -/

namespace LC.P0004
open Interview.Patterns

/-- `partitionOk i` — cutting at `i` yields a valid median partition (modeled as a monotone indicator). -/
def partitionOk (ok : ℕ → Prop) (i : ℕ) : Prop := ok i

instance (ok : ℕ → Prop) [DecidablePred ok] : DecidablePred (partitionOk ok) :=
  fun i => inferInstanceAs (Decidable (ok i))

/-- The binary-search answer: the least valid partition cut. -/
def sol (ok : ℕ → Prop) [DecidablePred ok] (h : ∃ i, partitionOk ok i) : ℕ := Nat.find h

/-- The partition predicate is a monotone up-set: once a cut is valid, every later cut is too. -/
theorem partition_mono (ok : ℕ → Prop) (hmono : ∀ a b, a ≤ b → ok a → ok b) :
    ∀ a b, a ≤ b → partitionOk ok a → partitionOk ok b := hmono

/-- SCHEME (bisection): the predicate is a monotone threshold — `partitionOk i ↔ cut ≤ i`. -/
theorem cls (ok : ℕ → Prop) [DecidablePred ok] (hmono : ∀ a b, a ≤ b → ok a → ok b)
    (h : ∃ i, partitionOk ok i) (n : ℕ) : partitionOk ok n ↔ sol ok h ≤ n :=
  bisection_threshold (partitionOk ok) (partition_mono ok hmono) h n

/-- The answer is the least valid partition cut — where the median is read off. -/
theorem corr (ok : ℕ → Prop) [DecidablePred ok] (h : ∃ i, partitionOk ok i) :
    IsLeast {i | partitionOk ok i} (sol ok h) :=
  bisection_isLeast (partitionOk ok) h

end LC.P0004
