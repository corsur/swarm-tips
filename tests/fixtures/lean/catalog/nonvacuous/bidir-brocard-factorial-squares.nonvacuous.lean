-- SPDX-License-Identifier: Apache-2.0
import Mathlib.Data.Nat.Factorial.Basic

/-- Machine-checked non-vacuity witness for `bidir-brocard-factorial-squares.lean`:
    all three known Brown numbers `(4, 5)`, `(5, 11)`, `(7, 71)` satisfy `n! + 1 = m^2`,
    and non-solutions like `(6, 26)` fail the equation. -/
theorem nonvacuous_brocard :
    (Nat.factorial 4 + 1 = 5 ^ 2) ∧
    (Nat.factorial 5 + 1 = 11 ^ 2) ∧
    (Nat.factorial 7 + 1 = 71 ^ 2) ∧
    (Nat.factorial 6 + 1 ≠ 26 ^ 2) := by
  refine ⟨by decide, by decide, by decide, by decide⟩
