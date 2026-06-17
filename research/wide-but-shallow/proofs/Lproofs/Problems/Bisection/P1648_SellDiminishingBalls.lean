import Lproofs.Schemes.Bisect

/-! @lc 1648 | name:Sell Diminishing-Valued Colored Balls | scheme:bisection | family:binary-search |
    complexity:O(n log) | source:https://leetcode.com/problems/sell-diminishing-valued-colored-balls/

    Each color with `inv` balls has balls valued `inv, inv−1, …, 1`. The number of balls worth at
    least `T` is `Σ max(0, inv−T+1)`, which is non-increasing in `T`. So "no more than `orders` balls
    are worth ≥ `T`" is a monotone threshold, and the accepted solution binary-searches the cutoff
    value `T`. CLASSIFICATION ONLY: we certify the bisection structure (the count predicate is a
    monotone threshold, so the cutoff is its `Nat.find`); we drop the resulting profit computation. -/

namespace LC.P1648
open Interview.Patterns

/-- Number of balls worth at least `T` across all colors (the real per-color count `(inv+1) ∸ T`). -/
def ballsGE (inventory : List ℕ) (T : ℕ) : ℕ := (inventory.map (fun i => (i + 1) - T)).sum

/-- `cutoff T` = at most `orders` balls are worth at least `T`. -/
def cutoff (inventory : List ℕ) (orders T : ℕ) : Prop := ballsGE inventory T ≤ orders

instance (inventory : List ℕ) (orders : ℕ) : DecidablePred (cutoff inventory orders) :=
  fun T => inferInstanceAs (Decidable (ballsGE inventory T ≤ orders))

/-- The accepted binary-search answer: the least value cutoff at which ≤ `orders` balls remain. -/
def sol (inventory : List ℕ) (orders : ℕ) (h : ∃ T, cutoff inventory orders T) : ℕ := Nat.find h

/-- `ballsGE` is non-increasing in `T`: a higher cutoff leaves at most as many balls (real, non-vacuous). -/
theorem ballsGE_anti (inventory : List ℕ) {a b : ℕ} (hab : a ≤ b) :
    ballsGE inventory b ≤ ballsGE inventory a := by
  induction inventory with
  | nil => simp [ballsGE]
  | cons i is ih =>
    simp only [ballsGE, List.map_cons, List.sum_cons]
    exact Nat.add_le_add (Nat.sub_le_sub_left hab _) ih

/-- `cutoff` is a monotone (up-set) threshold — raising `T` only reduces the surviving ball count. -/
theorem cutoff_mono (inventory : List ℕ) (orders : ℕ) :
    ∀ a b, a ≤ b → cutoff inventory orders a → cutoff inventory orders b :=
  fun _ _ hab ha => le_trans (ballsGE_anti inventory hab) ha

/-- SCHEME (bisection): the count predicate is a monotone threshold —
    `cutoff n ↔ (least cutoff value) ≤ n` — exactly what the binary search exploits. -/
theorem cls (inventory : List ℕ) (orders : ℕ) (h : ∃ T, cutoff inventory orders T) (n : ℕ) :
    cutoff inventory orders n ↔ sol inventory orders h ≤ n :=
  bisection_threshold (cutoff inventory orders) (cutoff_mono inventory orders) h n

/-- The answer is the least cutoff value (the threshold). -/
theorem corr (inventory : List ℕ) (orders : ℕ) (h : ∃ T, cutoff inventory orders T) :
    IsLeast {T | cutoff inventory orders T} (sol inventory orders h) :=
  bisection_isLeast (cutoff inventory orders) h

end LC.P1648
