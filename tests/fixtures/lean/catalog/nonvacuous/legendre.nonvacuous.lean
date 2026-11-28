-- SPDX-License-Identifier: Apache-2.0
import Mathlib.Data.Nat.Prime.Basic

/-- Machine-checked non-vacuity witness for `legendre.lean`:
    concrete positive integers `n = 1, 2, 3, 4` strictly bracket primes between `n^2` and `(n+1)^2`. -/
theorem nonvacuous_legendre :
    (∃ p : ℕ, 1 ^ 2 < p ∧ p < (1 + 1) ^ 2 ∧ Nat.Prime p) ∧
    (∃ p : ℕ, 2 ^ 2 < p ∧ p < (2 + 1) ^ 2 ∧ Nat.Prime p) ∧
    (∃ p : ℕ, 3 ^ 2 < p ∧ p < (3 + 1) ^ 2 ∧ Nat.Prime p) ∧
    (∃ p : ℕ, 4 ^ 2 < p ∧ p < (4 + 1) ^ 2 ∧ Nat.Prime p) := by
  refine ⟨⟨2, by decide⟩, ⟨5, by decide⟩, ⟨11, by decide⟩, ⟨17, by decide⟩⟩
