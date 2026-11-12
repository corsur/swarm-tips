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

/-- A positive integer `n` is squarefree if no prime `p ≤ n` has `p * p ∣ n`. -/
def isSquarefreePos (n : ℕ) : Prop :=
  0 < n ∧ ∀ p ≤ n, Nat.Prime p → ¬ (p * p ∣ n)

/-- Alaoglu–Erdős superabundant integer: `n > 0` strictly maximizes `σ(m)/m` over `[1, n]`. -/
def isSuperabundant (n : ℕ) : Prop :=
  0 < n ∧ ∀ m : ℕ, 0 < m → m < n → sigmaDivisors m * (n : ℝ) < sigmaDivisors n * (m : ℝ)

/-- Chain A [Rung 4/4 → RH Lagarias Equivalence] (Narrowed Even Non-Squarefree Superabundant Core):
    Restricts Lagarias's RH equivalence to the finite base `[1, 5040]` plus even, non-squarefree
    superabundant integers `n > 5040` (Akbary–Friggstad 2009). Combined with Rungs A1–A3 via
    `proofs/millennium-chains/rh_lagarias_chain.lean`, this implies full RH. -/
def statementProp : Prop :=
  (∀ (n : ℕ), 1 ≤ n → n ≤ 5040 → lagariasBound n) ∧
    (∀ (n : ℕ), 5040 < n → isSuperabundant n → n % 2 = 0 → ¬ isSquarefreePos n → lagariasBound n)
