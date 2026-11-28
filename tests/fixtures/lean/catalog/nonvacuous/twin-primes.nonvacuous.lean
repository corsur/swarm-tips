-- SPDX-License-Identifier: Apache-2.0
import Mathlib.Data.Nat.Prime.Basic

/-- Machine-checked non-vacuity witness for `twin-primes.lean`:
    concrete lower bounds admit twin prime pairs `(3, 5)`, `(11, 13)`, `(101, 103)`. -/
theorem nonvacuous_twin_primes :
    (∃ p : ℕ, 0 ≤ p ∧ Nat.Prime p ∧ Nat.Prime (p + 2)) ∧
    (∃ p : ℕ, 10 ≤ p ∧ Nat.Prime p ∧ Nat.Prime (p + 2)) ∧
    (∃ p : ℕ, 100 ≤ p ∧ Nat.Prime p ∧ Nat.Prime (p + 2)) := by
  refine ⟨⟨3, by decide, by decide, by decide⟩,
          ⟨11, by decide, by decide, by decide⟩,
          ⟨101, by decide, by decide, by decide⟩⟩
