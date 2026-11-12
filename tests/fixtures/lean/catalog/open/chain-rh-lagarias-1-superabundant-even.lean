import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

open scoped BigOperators

/-- Sum of positive divisors σ(n) = ∑_{d ∈ [1, n], d ∣ n} d. -/
noncomputable def sigmaDivisors (n : ℕ) : ℝ :=
  ∑ d ∈ (Finset.Icc 1 n).filter (fun d => d ∣ n), (d : ℝ)

/-- The n-th harmonic number H_n = ∑_{k=1}^n 1/k. -/
noncomputable def harmonicNum (n : ℕ) : ℝ :=
  ∑ k ∈ Finset.Icc 1 n, (1 : ℝ) / (k : ℝ)

/-- Lagarias's elementary inequality at n: σ(n) ≤ H_n + exp(H_n) · log(H_n). -/
def lagariasBound (n : ℕ) : Prop :=
  sigmaDivisors n ≤ harmonicNum n + Real.exp (harmonicNum n) * Real.log (harmonicNum n)

/-- Alaoglu–Erdős superabundant integer: `n > 0` strictly maximizes `σ(m)/m` over `[1, n]`,
    written cleared of denominators as `σ(m) · n < σ(n) · m` for all `1 ≤ m < n`. -/
def isSuperabundant (n : ℕ) : Prop :=
  0 < n ∧ ∀ m : ℕ, 0 < m → m < n → sigmaDivisors m * (n : ℝ) < sigmaDivisors n * (m : ℝ)

/-- Chain A [Rung 1/4 → RH Lagarias Equivalence] (Alaoglu–Erdős Parity Lemma, 1944):
    Proves the base case `lagariasBound 1` together with the structural parity theorem that
    every superabundant integer `n ≥ 2` is even (`n % 2 = 0`), by swapping an odd prime power
    `p^a` for `2^a` in the multiplicative divisor-sum formula. -/
def statementProp : Prop :=
  lagariasBound 1 ∧ ∀ (n : ℕ), 2 ≤ n → isSuperabundant n → n % 2 = 0
