import Mathlib.Data.Nat.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

open scoped BigOperators
open Classical

/-- Sum of positive divisors σ(n) = ∑_{d ∈ [1, n], d ∣ n} d. -/
noncomputable def sigmaDivisors (n : ℕ) : ℝ :=
  ∑ d ∈ (Finset.Icc 1 n).filter (fun d => d ∣ n), (d : ℝ)

/-- The n-th harmonic number H_n = ∑_{k=1}^n 1/k. -/
noncomputable def harmonicNum (n : ℕ) : ℝ :=
  ∑ k ∈ Finset.Icc 1 n, (1 : ℝ) / (k : ℝ)

/-- Millennium Problem (Riemann Hypothesis — Lagarias's Exact Elementary Equivalence, 2002):
    For every integer n ≥ 1, σ(n) ≤ H_n + exp(H_n) · log(H_n).
    By Lagarias (Amer. Math. Monthly 109, 2002, Theorem 1.1), this elementary inequality
    is strictly equivalent to the Clay Millennium Riemann Hypothesis.
    Proving `statementProp` proves RH; proving `¬ statementProp` (by exhibiting a single
    counterexample n ≥ 1) disproves RH. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 1 ≤ n →
    sigmaDivisors n ≤ harmonicNum n + Real.exp (harmonicNum n) * Real.log (harmonicNum n)
