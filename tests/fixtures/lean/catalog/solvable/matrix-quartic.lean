import Mathlib.Data.Real.Basic
import Mathlib.Tactic.Ring

/-- Quartic trace moment binomial expansion identity for two eigenvalues. -/
def statementProp : Prop :=
  ∀ (l1 l2 : ℝ),
    (l1 + l2) ^ 4 = l1 ^ 4 + 4 * l1 ^ 3 * l2 + 6 * l1 ^ 2 * l2 ^ 2 + 4 * l1 * l2 ^ 3 + l2 ^ 4
