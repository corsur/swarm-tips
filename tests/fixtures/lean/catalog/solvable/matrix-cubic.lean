import Mathlib.Data.Real.Basic
import Mathlib.Tactic.Ring

/-- Cubic trace moment algebraic identity for two eigenvalues. -/
def statementProp : Prop :=
  ∀ (l1 l2 : ℝ),
    (l1 + l2) * (l1 ^ 2 - l1 * l2 + l2 ^ 2) = l1 ^ 3 + l2 ^ 3
