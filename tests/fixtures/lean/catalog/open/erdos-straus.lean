import Mathlib.Data.Nat.Prime.Basic

def statementProp : Prop :=
  ∀ n : ℕ, 2 ≤ n → ∃ a b c : ℕ, 0 < a ∧ 0 < b ∧ 0 < c ∧
    4 * (a * b * c) = n * (b * c + a * c + a * b)
