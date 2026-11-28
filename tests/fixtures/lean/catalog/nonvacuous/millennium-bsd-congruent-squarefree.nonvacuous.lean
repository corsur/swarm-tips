-- SPDX-License-Identifier: Apache-2.0
import Mathlib.Data.Nat.Prime.Basic

private def isSquarefreePosW (n : ℕ) : Prop :=
  0 < n ∧ ∀ p ≤ n, Nat.Prime p → ¬ (p * p ∣ n)

private def isCongruentNumberW (n : ℕ) : Prop :=
  ∃ (a b c d : ℕ), 0 < a ∧ 0 < b ∧ 0 < c ∧ 0 < d ∧
    a ^ 2 + b ^ 2 = c ^ 2 ∧ a * b = 2 * n * (d ^ 2)

/-- Machine-checked non-vacuity witness for `millennium-bsd-congruent-squarefree.lean`:
    exhibits concrete squarefree witnesses in all three residue classes `5, 6, 7 (mod 8)`
    and verifies their rational right-triangle congruent-number witnesses. -/
theorem nonvacuous_bsd_congruent_squarefree :
    (isSquarefreePosW 5 ∧ 5 % 8 = 5 ∧ isCongruentNumberW 5) ∧
    (isSquarefreePosW 6 ∧ 6 % 8 = 6 ∧ isCongruentNumberW 6) ∧
    (isSquarefreePosW 7 ∧ 7 % 8 = 7 ∧ isCongruentNumberW 7) := by
  refine ⟨⟨by unfold isSquarefreePosW; decide, by decide, ⟨9, 40, 41, 6, by decide⟩⟩,
          ⟨by unfold isSquarefreePosW; decide, by decide, ⟨3, 4, 5, 1, by decide⟩⟩,
          ⟨by unfold isSquarefreePosW; decide, by decide, ⟨175, 288, 337, 60, by decide⟩⟩⟩
