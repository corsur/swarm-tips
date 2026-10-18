import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

open scoped BigOperators
open Classical

/-- A positive integer `n` is squarefree if no prime `p` has `p² ∣ n`. -/
def isSquarefreeNat (n : ℕ) : Prop :=
  0 < n ∧ ∀ p ≤ n, Nat.Prime p → ¬ (p * p ∣ n)

/-- Number of distinct prime factors ω(n) of `n`. -/
noncomputable def primeFactorCount (n : ℕ) : ℕ :=
  ((Finset.range (n + 1)).filter (fun p => Nat.Prime p ∧ p ∣ n)).card

/-- The classical Möbius function μ(n):
    • `0` if `n = 0` or `n` is divisible by a prime square `p²`,
    • `(-1)^{ω(n)}` if `n ≥ 1` is squarefree. -/
noncomputable def mobiusInt (n : ℕ) : ℤ :=
  if isSquarefreeNat n then (-1 : ℤ) ^ (primeFactorCount n) else 0

/-- The Mertens function M(N) = ∑_{n=1}^N μ(n) (with μ(0) = 0). -/
noncomputable def mertensSum (N : ℕ) : ℤ :=
  ∑ n ∈ Finset.range (N + 1), mobiusInt n

/-- Millennium Problem (Riemann Hypothesis — Littlewood–Mertens Quasi-RH Avenue):
    Unconditional power saving M(N) = O(N^{3/4}) on the Mertens sum, stated in pure
    integer arithmetic as M(N)^4 ≤ C · N^3 for a single universal constant C > 0
    quantified OUTSIDE ∀ N ≥ 1. By Littlewood (1912), this is strictly equivalent to
    a zero-free strip Re(s) > 3/4 for ζ(s) (with full RH corresponding to M(N) = O(N^{1/2+ε})). -/
def statementProp : Prop :=
  ∃ (C : ℤ), 1 ≤ C ∧
    ∀ (N : ℕ), 1 ≤ N →
      (mertensSum N) ^ 4 ≤ C * ((N : ℤ) ^ 3)
