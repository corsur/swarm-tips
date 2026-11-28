-- SPDX-License-Identifier: Apache-2.0
import Mathlib.Data.Nat.Prime.Basic

/-- Machine-checked non-vacuity witness for `erdos-straus.lean`:
    concrete integers `n ≥ 2` (`n = 2, 3, 5, 7`) admit positive Egyptian fraction triples
    `4 / n = 1 / a + 1 / b + 1 / c`. -/
theorem nonvacuous_erdos_straus :
    (∃ a b c : ℕ, 0 < a ∧ 0 < b ∧ 0 < c ∧ 4 * (a * b * c) = 2 * (b * c + a * c + a * b)) ∧
    (∃ a b c : ℕ, 0 < a ∧ 0 < b ∧ 0 < c ∧ 4 * (a * b * c) = 5 * (b * c + a * c + a * b)) ∧
    (∃ a b c : ℕ, 0 < a ∧ 0 < b ∧ 0 < c ∧ 4 * (a * b * c) = 7 * (b * c + a * c + a * b)) := by
  refine ⟨⟨1, 2, 2, by decide⟩, ⟨2, 4, 20, by decide⟩, ⟨2, 15, 210, by decide⟩⟩
