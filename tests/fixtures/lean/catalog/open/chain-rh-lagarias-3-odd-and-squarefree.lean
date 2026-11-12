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

/-- Chain A [Rung 3/4 → RH Lagarias Equivalence] (Choie–Lichiardopol–Moree–Solé 2007):
    Lagarias's Riemann Hypothesis inequality holds unconditionally for every odd positive
    integer and every squarefree positive integer, eliminating both infinite families from
    the open Millennium target `millennium-rh-lagarias-divisor`. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 1 ≤ n → (n % 2 = 1 ∨ isSquarefreePos n) → lagariasBound n
