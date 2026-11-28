-- SPDX-License-Identifier: Apache-2.0
import Mathlib.Data.Nat.Prime.Basic

/-- Machine-checked non-vacuity witness for `goldbach.lean`:
    concrete even integers `n > 2` satisfy the preconditions and admit prime-pair decompositions. -/
theorem nonvacuous_goldbach :
    (∃ n : ℕ, 2 < n ∧ Even n) ∧
    (∃ p q : ℕ, Nat.Prime p ∧ Nat.Prime q ∧ 4 = p + q) ∧
    (∃ p q : ℕ, Nat.Prime p ∧ Nat.Prime q ∧ 10 = p + q) := by
  refine ⟨⟨4, by decide, ⟨2, by decide⟩⟩, ?_, ?_⟩
  · exact ⟨2, 2, by decide, by decide, by decide⟩
  · exact ⟨3, 7, by decide, by decide, by decide⟩
