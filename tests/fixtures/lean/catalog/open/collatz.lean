import Mathlib.Logic.Function.Iterate

def collatz (n : ℕ) : ℕ := if n % 2 = 0 then n / 2 else 3 * n + 1

def statementProp : Prop := ∀ n : ℕ, 0 < n → ∃ k : ℕ, collatz^[k] n = 1
