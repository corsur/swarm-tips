import Mathlib.Data.Nat.Prime.Basic

def statementProp : Prop :=
  ∀ n : ℕ, 2 < n → Even n → ∃ p q, Nat.Prime p ∧ Nat.Prime q ∧ n = p + q
