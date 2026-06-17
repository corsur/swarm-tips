import Lproofs.Patterns

/-! @lc 2214 | name:Minimum Health to Beat Game | scheme:fold | family:dp-linear | complexity:O(n) |
    source:https://leetcode.com/problems/minimum-health-to-beat-game/

    Survive every level; armor may cancel up to `armor` damage on a single level. The accepted answer
    is `total damage − min(armor, biggest hit) + 1`, computed in one streaming pass that accumulates
    the running `(sum, max)`. CLASSIFICATION: a single left fold with a bounded `(sum, max)` scalar
    accumulator (the "dp-linear" bookkeeping label notwithstanding — there is no table). NON-VACUITY:
    we prove the accumulator equals `(total sum, running max)`, pinning the step to real aggregation. -/

namespace LC.P2214
open Interview.Patterns

/-- One pass step: accumulate the total and the running maximum. -/
def step (acc : ℕ × ℕ) (d : ℕ) : ℕ × ℕ := (acc.1 + d, max acc.2 d)

/-- Stream the damage list into `(sum, max)`. -/
def scan (damage : List ℕ) : ℕ × ℕ := damage.foldl step (0, 0)

/-- The accepted answer: total damage minus the largest cancellable hit, plus one. -/
def sol (damage : List ℕ) (armor : ℕ) : ℕ := (scan damage).1 - min armor (scan damage).2 + 1

/-- SCHEME (fold): the pass is a single left fold with a bounded `(sum, max)` accumulator. -/
theorem cls : IsFold (fun damage : List ℕ => damage.foldl step (0, 0)) :=
  ⟨step, (0, 0), fun _ => rfl⟩

/-- The fold splits over the pair: it is exactly `(running sum, running max)`. -/
theorem scan_eq (damage : List ℕ) (s m : ℕ) :
    damage.foldl step (s, m) = (damage.foldl (· + ·) s, damage.foldl max m) := by
  induction damage generalizing s m with
  | nil => rfl
  | cons d ds ih => simp only [List.foldl_cons, step]; exact ih (s + d) (max m d)

/-- NON-VACUITY: the accumulator is the genuine total and running maximum of the damage list. -/
theorem corr (damage : List ℕ) :
    (scan damage).1 = damage.foldl (· + ·) 0 ∧ (scan damage).2 = damage.foldl max 0 := by
  rw [scan, scan_eq]
  exact ⟨rfl, rfl⟩

end LC.P2214
