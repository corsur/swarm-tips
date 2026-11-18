/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Data.Nat.Factorial.Basic
import Mathlib.Data.Rat.Cast.Order
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

/-- Computable natural-number divisor sum σ_ℕ(n) = ∑_{d ∈ [1, n], d ∣ n} d. -/
def sigmaNat (n : ℕ) : ℕ :=
  Finset.sum ((Finset.Icc 1 n).filter (fun d => d ∣ n)) (fun d => d)

#guard sigmaNat 1 = 1
#guard sigmaNat 6 = 12
#guard sigmaNat 12 = 28

/-- Computable rational divisor sum σ_ℚ(n) = ∑_{d ∈ [1, n], d ∣ n} d. -/
def sigmaRat (n : ℕ) : ℚ :=
  Finset.sum ((Finset.Icc 1 n).filter (fun d => d ∣ n)) (fun d => (d : ℚ))

#guard sigmaRat 1 = (1 : ℚ)
#guard sigmaRat 6 = (12 : ℚ)
#guard sigmaRat 12 = (28 : ℚ)

/-- Sum of positive divisors σ(n) = ∑_{d ∈ [1, n], d ∣ n} d over ℝ. -/
noncomputable def sigmaDivisors (n : ℕ) : ℝ :=
  Finset.sum ((Finset.Icc 1 n).filter (fun d => d ∣ n)) (fun d => (d : ℝ))

/-- Computable rational n-th harmonic number H_ℚ(n) = ∑_{k=1}^n 1/k. -/
def harmonicRat (n : ℕ) : ℚ :=
  Finset.sum (Finset.Icc 1 n) (fun k => (1 : ℚ) / (k : ℚ))

#guard harmonicRat 1 = (1 : ℚ)
#guard harmonicRat 2 = (3 : ℚ) / 2
#guard harmonicRat 3 = (11 : ℚ) / 6

/-- The n-th harmonic number H_n = ∑_{k=1}^n 1/k over ℝ. -/
noncomputable def harmonicNum (n : ℕ) : ℝ :=
  Finset.sum (Finset.Icc 1 n) (fun k => (1 : ℝ) / (k : ℝ))

/-- Lagarias's elementary inequality at n: σ(n) ≤ H_n + exp(H_n) · log(H_n). -/
def lagariasBound (n : ℕ) : Prop :=
  sigmaDivisors n ≤ harmonicNum n + Real.exp (harmonicNum n) * Real.log (harmonicNum n)

/-- Alaoglu–Erdős superabundant integer: `n > 0` strictly maximizes `σ(m)/m` over `[1, n]`,
    written cleared of denominators as `σ(m) · n < σ(n) · m` for all `1 ≤ m < n`. -/
def isSuperabundant (n : ℕ) : Prop :=
  0 < n ∧ ∀ m : ℕ, 0 < m → m < n → sigmaDivisors m * (n : ℝ) < sigmaDivisors n * (m : ℝ)

/-- A positive integer `n` is squarefree if no prime `p ≤ n` has `p * p ∣ n`. -/
def isSquarefreePos (n : ℕ) : Prop :=
  0 < n ∧ ∀ p ≤ n, Nat.Prime p → ¬ (p * p ∣ n)

/-- Constructive rational Taylor upper bound on exp(x) of degree M:
    ∑_{j=0}^{M-1} x^j / j! + 2 · x^M / M! (valid upper bound when 0 ≤ x and 2x ≤ M). -/
def expUpperRat (x : ℚ) (M : ℕ) : ℚ :=
  Finset.sum (Finset.range M) (fun j => (x ^ j) / (Nat.factorial j : ℚ)) +
    2 * (x ^ M) / (Nat.factorial M : ℚ)

#guard expUpperRat 0 3 = (1 : ℚ)
#guard (2 : ℚ) < expUpperRat 1 4

/-- Constructive rational hyperbolic-arctanh upper bound on log(x) of order M for x ≥ 1,
    with t = (x - 1) / (x + 1) ∈ [0, 1):
    2 · ∑_{j=0}^{M-1} t^{2j+1}/(2j+1) + (x + 1) · t^{2M+1}/(2M + 1). -/
def logUpperRat (x : ℚ) (M : ℕ) : ℚ :=
  let t : ℚ := (x - 1) / (x + 1)
  (2 * Finset.sum (Finset.range M) (fun j => (t ^ (2 * j + 1)) / ((2 * j + 1 : ℕ) : ℚ))) +
    (x + 1) * (t ^ (2 * M + 1)) / ((2 * M + 1 : ℕ) : ℚ)

#guard logUpperRat 1 3 = (0 : ℚ)

/-- Purely rational Lagarias upper envelope U_Lag(n, K) ∈ ℚ at truncation M = 2n + K. -/
def lagariasEnvelopeRat (n K : ℕ) : ℚ :=
  let h := harmonicRat n
  let M := 2 * n + K
  h + expUpperRat h M * logUpperRat h M

#guard lagariasEnvelopeRat 1 1 = (1 : ℚ)
#guard sigmaRat 2 ≤ lagariasEnvelopeRat 2 1
#guard sigmaRat 6 ≤ lagariasEnvelopeRat 6 1
