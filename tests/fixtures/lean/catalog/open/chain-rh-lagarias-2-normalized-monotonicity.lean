import Mathlib.Data.Nat.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

open scoped BigOperators

/-- The n-th harmonic number H_n = ∑_{k=1}^n 1/k. -/
noncomputable def harmonicNum (n : ℕ) : ℝ :=
  ∑ k ∈ Finset.Icc 1 n, (1 : ℝ) / (k : ℝ)

/-- Chain A [Rung 2/4 → RH Lagarias Equivalence] (Normalized Lagarias RHS Monotonicity):
    For all integers `1 ≤ k ≤ n`, the normalized Lagarias upper-bound sequence
    `g(k) = (H_k + exp(H_k) · log(H_k)) / k` is monotonically non-decreasing (cleared of
    denominators). Unlike Robin's `e^γ log(log k)` bound, `g(k)` has zero finite dips on
    `[1, ∞)`, powering minimal-counterexample descent from any `n` to a superabundant witness. -/
def statementProp : Prop :=
  ∀ (k n : ℕ), 1 ≤ k → k ≤ n →
    (harmonicNum k + Real.exp (harmonicNum k) * Real.log (harmonicNum k)) * (n : ℝ) ≤
      (harmonicNum n + Real.exp (harmonicNum n) * Real.log (harmonicNum n)) * (k : ℝ)
