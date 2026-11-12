import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Tactic.Linarith

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

/-- Chain C [Rung 3/4 → RH Analytic Mertens Hierarchy] (Landau 1899 PNT Equivalence):
    Sublinear cancellation `M(N) = o(N)` of the Mertens function (`∀ k ≥ 1, ∃ N₀, ∀ N ≥ N₀,
    k · |M(N)| ≤ N`), strictly equivalent to the Prime Number Theorem and non-vanishing of
    `ζ(1 + it)` on the vertical boundary `Re(s) = 1`. -/
def statementProp : Prop :=
  ∀ (k : ℕ), 1 ≤ k → ∃ (N₀ : ℕ), ∀ (N : ℕ), N₀ ≤ N → (k : ℤ) * |mertensSum N| ≤ (N : ℤ)
