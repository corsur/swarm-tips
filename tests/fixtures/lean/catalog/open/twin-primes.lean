import Mathlib.Data.Nat.Prime.Basic

def statementProp : Prop := ∀ n : ℕ, ∃ p, n ≤ p ∧ Nat.Prime p ∧ Nat.Prime (p + 2)
