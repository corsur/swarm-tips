import Mathlib.Data.Nat.Prime.Basic

def statementProp : Prop :=
  ∀ n : ℕ, 0 < n → ∃ p, n ^ 2 < p ∧ p < (n + 1) ^ 2 ∧ Nat.Prime p
