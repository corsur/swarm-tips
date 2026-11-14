import Mathlib.Data.Nat.Factorial.Basic
import Mathlib.Data.Rat.Cast.Order
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

open scoped BigOperators

/-- Rational divisor sum σ_ℚ(n) = ∑_{d ∈ [1, n], d ∣ n} d. -/
def sigmaRat (n : ℕ) : ℚ :=
  ∑ d ∈ (Finset.Icc 1 n).filter (fun d => d ∣ n), (d : ℚ)

/-- Rational n-th harmonic number H_ℚ(n) = ∑_{k=1}^n 1/k. -/
def harmonicRat (n : ℕ) : ℚ :=
  ∑ k ∈ Finset.Icc 1 n, (1 : ℚ) / (k : ℚ)

/-- Constructive rational Taylor upper bound on exp(x) of degree M:
    ∑_{j=0}^{M-1} x^j / j! + 2 · x^M / M! (valid upper bound when 0 ≤ x and 2x ≤ M). -/
def expUpperRat (x : ℚ) (M : ℕ) : ℚ :=
  (∑ j ∈ Finset.range M, (x ^ j) / (Nat.factorial j : ℚ)) +
    2 * (x ^ M) / (Nat.factorial M : ℚ)

/-- Constructive rational hyperbolic-arctanh upper bound on log(x) of order M for x ≥ 1,
    with t = (x - 1) / (x + 1) ∈ [0, 1):
    2 · ∑_{j=0}^{M-1} t^{2j+1}/(2j+1) + (x + 1) · t^{2M+1}/(2M + 1). -/
def logUpperRat (x : ℚ) (M : ℕ) : ℚ :=
  let t : ℚ := (x - 1) / (x + 1)
  (2 * ∑ j ∈ Finset.range M, (t ^ (2 * j + 1)) / ((2 * j + 1 : ℕ) : ℚ)) +
    (x + 1) * (t ^ (2 * M + 1)) / ((2 * M + 1 : ℕ) : ℚ)

/-- Purely rational Lagarias upper envelope U_Lag(n, K) ∈ ℚ at truncation M = 2n + K. -/
def lagariasEnvelopeRat (n K : ℕ) : ℚ :=
  let h := harmonicRat n
  let M := 2 * n + K
  h + expUpperRat h M * logUpperRat h M

/-- Real divisor sum σ(n) matching `millennium-rh-lagarias-divisor.lean`. -/
noncomputable def sigmaDivisors (n : ℕ) : ℝ :=
  ∑ d ∈ (Finset.Icc 1 n).filter (fun d => d ∣ n), (d : ℝ)

/-- Real n-th harmonic number H_n matching `millennium-rh-lagarias-divisor.lean`. -/
noncomputable def harmonicNum (n : ℕ) : ℝ :=
  ∑ k ∈ Finset.Icc 1 n, (1 : ℝ) / (k : ℝ)

/-- Lagarias's transcendental inequality at n over ℝ: σ(n) ≤ H_n + exp(H_n) · log(H_n). -/
def lagariasBound (n : ℕ) : Prop :=
  sigmaDivisors n ≤ harmonicNum n + Real.exp (harmonicNum n) * Real.log (harmonicNum n)

/-- Track E [Rung 1/4 → Π₁⁰ RH Metamathematics] (Rational Π₁⁰ Envelope Equivalence):
    Eliminates ℝ, Real.exp, and Real.log from Lagarias's RH criterion without Schanuel's
    Conjecture by proving that for every n ≥ 1, `lagariasBound n` holds iff the purely
    decidable rational inequality `sigmaRat n ≤ lagariasEnvelopeRat n K` holds for all K ≥ 1. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 1 ≤ n →
    (lagariasBound n ↔ ∀ (K : ℕ), 1 ≤ K → sigmaRat n ≤ lagariasEnvelopeRat n K)
