import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

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

/-- The Mertens function M(N) = ∑_{n=0}^N μ(n) (with μ(0) = 0). -/
def mertensSum (N : ℕ) : ℤ :=
  ∑ n ∈ Finset.range (N + 1), mobiusInt n

/-- Chain C [Rung 4/4 → RH Analytic Mertens Hierarchy] (Open Zero-Free Strip Re(s) > 0.99):
    Establishes a uniform power-saving Mertens bound `|M(N)| ≤ C^{1/100} · N^{99/100}` for all
    `N ≥ 1` (quantified with outer constant `∃ C ≥ 1`). Strictly implied by `millennium-rh-mertens-quasi`
    (`θ = 3/4`) and strictly implies Rung C3 (`PNT`), as kernel-verified in
    `proofs/millennium-chains/rh_analytic_chain.lean`. -/
def statementProp : Prop :=
  ∃ (C : ℤ), 1 ≤ C ∧ ∀ (N : ℕ), 1 ≤ N → (mertensSum N) ^ 100 ≤ C * ((N : ℤ) ^ 99)
