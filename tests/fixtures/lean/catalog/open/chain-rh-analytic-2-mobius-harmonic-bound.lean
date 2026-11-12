import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Tactic.NormNum

open scoped BigOperators

/-- Number of distinct prime factors ω(n) of `n`. -/
def primeFactorCount (n : ℕ) : ℕ :=
  ((Finset.range (n + 1)).filter (fun p => Nat.Prime p ∧ p ∣ n)).card

/-- Number of primes `p ≤ n` whose square `p²` divides `n`. -/
def squarePrimeFactorCount (n : ℕ) : ℕ :=
  ((Finset.range (n + 1)).filter (fun p => Nat.Prime p ∧ p * p ∣ n)).card

/-- The classical Möbius function μ(n) matching the Mertens hierarchy. -/
def mobiusInt (n : ℕ) : ℤ :=
  if 0 < n ∧ squarePrimeFactorCount n = 0 then (-1 : ℤ) ^ (primeFactorCount n) else 0

/-- Chain C [Rung 2/4 → RH Analytic Mertens Hierarchy] (Möbius Harmonic Cancellation Bound):
    For every integer `N ≥ 1`, the rational Möbius harmonic sum satisfies
    `|∑_{n=1}^N μ(n) / n| ≤ 1` (stated polynomially as `(∑_{k=0}^{N-1} μ(k+1)/(k+1))^2 ≤ 1`),
    derived from the Dirichlet convolution identity `∑_{n ≤ N} μ(n) · ⌊N / n⌋ = 1`. -/
def statementProp : Prop :=
  ∀ (N : ℕ), 1 ≤ N →
    (∑ k ∈ Finset.range N, (mobiusInt (k + 1) : ℚ) / ((k + 1 : ℕ) : ℚ)) ^ 2 ≤ (1 : ℚ)
