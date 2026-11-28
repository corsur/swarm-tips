-- SPDX-License-Identifier: Apache-2.0
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Order.Interval.Finset.Nat

/-- Machine-checked non-vacuity witness for `erdos-moser-power-sum.lean`:
    proves the hypothesis predicate (`1 ≤ n`, `2 ≤ m`, `∑_{k=1}^{m-1} k^n = m^n`) is
    satisfiable by `(m, n) = (3, 1)` and that non-solutions like `(4, 1)` exist. -/
theorem nonvacuous_erdos_moser :
    (∃ m n : ℕ, 1 ≤ n ∧ 2 ≤ m ∧
      Finset.sum (Finset.Ico 1 m) (fun k => k ^ n) = m ^ n ∧ m = 3 ∧ n = 1) ∧
    (Finset.sum (Finset.Ico 1 4) (fun k => k ^ 1) ≠ 4 ^ 1) := by
  refine ⟨⟨3, 1, by decide, by decide, by decide, rfl, rfl⟩, by decide⟩
