import Lproofs.Schemes.Fold

/-! @lc 636 | name:Exclusive Time of Functions | scheme:fold | family:pairing-stack | complexity:O(n) |
    source:https://leetcode.com/problems/exclusive-time-of-functions/

    A call stack attributes each inter-event interval to the function currently running, so the
    per-function exclusive times partition the whole timeline. Conservation: the total accounted
    exclusive time equals the elapsed span (last timestamp − first), with no time lost or
    double-counted. -/

namespace LC.P0636
open Interview.Patterns

/-- Total accounted time: the sum of consecutive inter-event gaps (the specification). -/
def totalExclusive : List ℕ → ℤ
  | [] => 0
  | [_] => 0
  | a :: b :: rest => ((b : ℤ) - a) + totalExclusive (b :: rest)

/-- One streaming step: carry `(previous timestamp?, accumulated gap-sum)`; on each new event add
    its gap to the running total. The accumulator is bounded (one `Option ℕ` and one `ℤ`). -/
def step (st : Option ℕ × ℤ) (t : ℕ) : Option ℕ × ℤ :=
  (some t, match st.1 with | none => st.2 | some p => st.2 + ((t : ℤ) - p))

/-- Accepted O(n) solution: a single left-to-right fold accumulating consecutive gaps. -/
def sol (times : List ℕ) : ℤ := (times.foldl step (none, 0)).2

/-- SCHEME (fold): `sol` itself is the left fold, with the bounded `(Option ℕ × ℤ)` accumulator. -/
theorem cls : IsFold (fun times : List ℕ => times.foldl step (none, 0)) :=
  ⟨step, (none, 0), fun _ => rfl⟩

/-- The fold from a known previous timestamp accumulates the telescoping gap-sum. -/
theorem fold_step (rest : List ℕ) (p : ℕ) (acc : ℤ) :
    (rest.foldl step (some p, acc)).2 = acc + totalExclusive (p :: rest) := by
  induction rest generalizing p acc with
  | nil => simp [totalExclusive]
  | cons b rest' ih =>
    change (rest'.foldl step (some b, acc + ((b : ℤ) - p))).2 = _
    rw [ih b (acc + ((b : ℤ) - p)), totalExclusive]; ring

/-- `sol` computes the specification `totalExclusive`. -/
theorem sol_eq (times : List ℕ) : sol times = totalExclusive times := by
  cases times with
  | nil => rfl
  | cons a rest =>
    change (rest.foldl step (some a, 0)).2 = _
    rw [fold_step rest a 0]; simp

/-- CORRECT (conservation): the total accounted exclusive time equals the elapsed span. -/
theorem corr : ∀ (a : ℕ) (rest : List ℕ),
    sol (a :: rest) = (rest.getLastD a : ℤ) - a := by
  intro a rest
  rw [sol_eq]
  induction rest generalizing a with
  | nil => simp [totalExclusive, List.getLastD]
  | cons b rest' ih =>
    rw [totalExclusive, ih b, List.getLastD_cons]
    ring

end LC.P0636
