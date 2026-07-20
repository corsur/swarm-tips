import Lproofs.Schemes.Bisect

/-! @lc 2861 | name:Maximum Number of Alloys | scheme:bisection | family:binary-search |
    complexity:O(log n) | source:https://leetcode.com/problems/maximum-number-of-alloys/

    With a fixed budget, find the most alloys a machine can produce; the cost to produce `t` alloys is
    non-decreasing in `t`, so the answer is found by binary search. CLASSIFICATION (bisection):
    `tooCostly t` ("producing `t` alloys exceeds the budget") is a monotone up-set in `t` — once `t`
    alloys are unaffordable, so is any larger run — so the binary search locates its least satisfying
    value, and the maximum affordable count is one below it. `cls` certifies the threshold structure;
    `corr` that `sol` is that least-unaffordable count (the boundary the search converges to). -/

namespace LC.P2861
open Interview.Patterns

/-- Producing `t` alloys costs more than the budget. `cost t` is the (monotone) cost of `t` alloys. -/
def tooCostly (cost : ℕ → ℕ) (budget t : ℕ) : Prop := budget < cost t

instance (cost : ℕ → ℕ) (budget : ℕ) : DecidablePred (tooCostly cost budget) :=
  fun t => inferInstanceAs (Decidable (budget < cost t))

/-- The binary-search boundary: the least count that exceeds the budget (max affordable is `sol - 1`). -/
def sol (cost : ℕ → ℕ) (budget : ℕ) (h : ∃ t, tooCostly cost budget t) : ℕ := Nat.find h

/-- Unaffordability is a monotone up-set when cost is monotone: more alloys never costs less. -/
theorem tooCostly_mono (cost : ℕ → ℕ) (hmono : Monotone cost) (budget : ℕ) :
    ∀ a b, a ≤ b → tooCostly cost budget a → tooCostly cost budget b :=
  fun _ _ hab ha => lt_of_lt_of_le ha (hmono hab)

/-- SCHEME (bisection): unaffordability is a monotone threshold — `tooCostly t ↔ (boundary) ≤ t`. -/
theorem cls (cost : ℕ → ℕ) (hmono : Monotone cost) (budget : ℕ)
    (h : ∃ t, tooCostly cost budget t) (n : ℕ) :
    tooCostly cost budget n ↔ sol cost budget h ≤ n :=
  bisection_threshold (tooCostly cost budget) (tooCostly_mono cost hmono budget) h n

/-- CORRECT: `sol` is the least alloy count that exceeds the budget — the boundary whose predecessor is
    the maximum number of alloys producible. -/
theorem corr (cost : ℕ → ℕ) (budget : ℕ) (h : ∃ t, tooCostly cost budget t) :
    IsLeast {t | tooCostly cost budget t} (sol cost budget h) :=
  bisection_isLeast (tooCostly cost budget) h


/-- TEST VECTOR (official example 1, best machine): composition [1,1,1], zero stock, metal costs
    [1,2,3] make `t` alloys cost `6t`; budget 15 — the least unaffordable count is 3, so the
    maximum producible is 2, the judge's answer. -/
theorem vec : sol (fun t => 6 * t) 15 ⟨3, by decide⟩ = 3 := by
  simp only [sol]
  rw [Nat.find_eq_iff]
  decide

end LC.P2861
