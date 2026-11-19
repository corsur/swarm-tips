/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Data.Nat.Factorial.Basic
import Mathlib.Data.Rat.Cast.Order
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.NumberTheory.ArithmeticFunction.Defs
import Mathlib.NumberTheory.ArithmeticFunction.Misc
import Mathlib.NumberTheory.Harmonic.Defs
import Mathlib.Order.Filter.Ultrafilter.Basic

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

/-- First-order arithmetic terms over de Bruijn variable indices `ℕ`. -/
inductive ArithTerm : Type where
  | var : ℕ → ArithTerm
  | const : ℕ → ArithTerm
  | add : ArithTerm → ArithTerm → ArithTerm
  | mul : ArithTerm → ArithTerm → ArithTerm

/-- Evaluate an `ArithTerm` in a standard variable assignment `env : ℕ → ℕ`. -/
def evalArithTerm (env : ℕ → ℕ) : ArithTerm → ℕ
  | .var v => env v
  | .const c => c
  | .add t₁ t₂ => evalArithTerm env t₁ + evalArithTerm env t₂
  | .mul t₁ t₂ => evalArithTerm env t₁ * evalArithTerm env t₂

/-- Extend a variable assignment at de Bruijn index `0` by `x`, shifting higher indices. -/
def consArithEnv {α : Type*} (x : α) (env : ℕ → α) : ℕ → α
  | 0 => x
  | v + 1 => env v

#guard evalArithTerm (fun v => v + 2) (.mul (.add (.var 0) (.const 3)) (.var 1)) = 15
#guard evalArithTerm (consArithEnv 10 (fun _ => 1)) (.add (.var 0) (.var 1)) = 11

/-- Bounded (`Δ₀`) first-order arithmetic formulas over de Bruijn variable indices `ℕ`.
    In `boundedForall bound body`, `bound` is evaluated in the outer environment `env`
    and `body` is evaluated in the shifted environment `consArithEnv y env` for `y ≤ bound`. -/
inductive BoundedArithFormula : Type where
  | eq : ArithTerm → ArithTerm → BoundedArithFormula
  | le : ArithTerm → ArithTerm → BoundedArithFormula
  | not : BoundedArithFormula → BoundedArithFormula
  | and : BoundedArithFormula → BoundedArithFormula → BoundedArithFormula
  | imp : BoundedArithFormula → BoundedArithFormula → BoundedArithFormula
  | boundedForall : ArithTerm → BoundedArithFormula → BoundedArithFormula

/-- Standard semantics `ℕ ⊨ φ[env]` for bounded (`Δ₀`) arithmetic formulas. -/
def evalBoundedFormula (env : ℕ → ℕ) : BoundedArithFormula → Prop
  | .eq t₁ t₂ => evalArithTerm env t₁ = evalArithTerm env t₂
  | .le t₁ t₂ => evalArithTerm env t₁ ≤ evalArithTerm env t₂
  | .not φ => ¬ evalBoundedFormula env φ
  | .and φ ψ => evalBoundedFormula env φ ∧ evalBoundedFormula env ψ
  | .imp φ ψ => evalBoundedFormula env φ → evalBoundedFormula env ψ
  | .boundedForall bound body =>
      ∀ y : ℕ, y ≤ evalArithTerm env bound → evalBoundedFormula (consArithEnv y env) body

/-- Computable Boolean checker mirroring `evalBoundedFormula` on standard `ℕ` assignments. -/
def evalBoundedFormulaBool (env : ℕ → ℕ) : BoundedArithFormula → Bool
  | .eq t₁ t₂ => decide (evalArithTerm env t₁ = evalArithTerm env t₂)
  | .le t₁ t₂ => decide (evalArithTerm env t₁ ≤ evalArithTerm env t₂)
  | .not φ => !(evalBoundedFormulaBool env φ)
  | .and φ ψ => evalBoundedFormulaBool env φ && evalBoundedFormulaBool env ψ
  | .imp φ ψ => !(evalBoundedFormulaBool env φ) || evalBoundedFormulaBool env ψ
  | .boundedForall bound body =>
      (List.range (evalArithTerm env bound + 1)).all (fun y => evalBoundedFormulaBool (consArithEnv y env) body)

#guard evalBoundedFormulaBool (fun _ => 4) (.boundedForall (.var 0) (.le (.var 0) (.var 1))) = true
#guard evalBoundedFormulaBool (fun _ => 2) (.boundedForall (.const 5) (.le (.var 0) (.var 1))) = false

/-- Ultrapower semantics `ℕ^ℕ / 𝒰 ⊨ φ[env]` for bounded (`Δ₀`) arithmetic formulas,
    where `boundedForall bound body` quantifies over all internal sequences `y : ℕ → ℕ`
    satisfying `{i | y i ≤ evalArithTerm (fun v => env v i) bound} ∈ 𝒰`. -/
def evalBoundedFormulaUltra (𝒰 : Ultrafilter ℕ) (env : ℕ → (ℕ → ℕ)) :
    BoundedArithFormula → Prop
  | .eq t₁ t₂ =>
      {i : ℕ | evalArithTerm (fun v => env v i) t₁ = evalArithTerm (fun v => env v i) t₂} ∈ 𝒰
  | .le t₁ t₂ =>
      {i : ℕ | evalArithTerm (fun v => env v i) t₁ ≤ evalArithTerm (fun v => env v i) t₂} ∈ 𝒰
  | .not φ => ¬ evalBoundedFormulaUltra 𝒰 env φ
  | .and φ ψ => evalBoundedFormulaUltra 𝒰 env φ ∧ evalBoundedFormulaUltra 𝒰 env ψ
  | .imp φ ψ => evalBoundedFormulaUltra 𝒰 env φ → evalBoundedFormulaUltra 𝒰 env ψ
  | .boundedForall bound body =>
      ∀ y : ℕ → ℕ,
        {i : ℕ | y i ≤ evalArithTerm (fun v => env v i) bound} ∈ 𝒰 →
        evalBoundedFormulaUltra 𝒰 (consArithEnv y env) body

/-- Canonical bridge lemma matching `sigmaNat` with `ArithmeticFunction.sigma 1`. -/
theorem bridge_sigmaNat (n : ℕ) :
    sigmaNat n = ArithmeticFunction.sigma 1 n := by
  rcases eq_or_ne n 0 with rfl | hn
  · simp [sigmaNat]
  · unfold sigmaNat
    rw [ArithmeticFunction.sigma_apply]
    have hset : (Finset.Icc 1 n).filter (fun d => d ∣ n) = n.divisors := by
      ext d
      simp only [Finset.mem_filter, Finset.mem_Icc, Nat.mem_divisors]
      exact ⟨fun h => ⟨h.2, hn⟩, fun h =>
        ⟨⟨Nat.succ_le_of_lt (Nat.pos_of_dvd_of_pos h.1 (Nat.pos_of_ne_zero hn)),
          Nat.le_of_dvd (Nat.pos_of_ne_zero hn) h.1⟩, h.1⟩⟩
    rw [hset]
    exact Finset.sum_congr rfl (fun d _ => (pow_one d).symm)

/-- Canonical bridge lemma matching `sigmaRat` with `(ArithmeticFunction.sigma 1 n : ℚ)`. -/
theorem bridge_sigmaRat (n : ℕ) :
    sigmaRat n = (ArithmeticFunction.sigma 1 n : ℚ) := by
  have hcast : sigmaRat n = (sigmaNat n : ℚ) := by
    unfold sigmaRat sigmaNat
    push_cast
    rfl
  rw [hcast, bridge_sigmaNat]

/-- Canonical bridge lemma matching `isSquarefreePos` with `0 < n ∧ Squarefree n`. -/
theorem bridge_isSquarefreePos (n : ℕ) :
    isSquarefreePos n ↔ (0 < n ∧ Squarefree n) := by
  unfold isSquarefreePos
  rw [Nat.squarefree_iff_prime_squarefree]
  constructor
  · rintro ⟨hpos, hno⟩
    refine ⟨hpos, fun p hp hpp => ?_⟩
    exact hno p (Nat.le_of_dvd hpos (dvd_trans ⟨p, rfl⟩ hpp)) hp hpp
  · rintro ⟨hpos, hsq⟩
    exact ⟨hpos, fun p _ hp => hsq p hp⟩

/-- Canonical bridge lemma matching `sigmaDivisors` with `(ArithmeticFunction.sigma 1 n : ℝ)`. -/
theorem bridge_sigmaDivisors (n : ℕ) :
    sigmaDivisors n = (ArithmeticFunction.sigma 1 n : ℝ) := by
  have hcast : sigmaDivisors n = (sigmaNat n : ℝ) := by
    unfold sigmaDivisors sigmaNat
    push_cast
    rfl
  rw [hcast, bridge_sigmaNat]

/-- Canonical bridge lemma matching `harmonicRat` with Mathlib's `harmonic`. -/
theorem bridge_harmonicRat (n : ℕ) :
    harmonicRat n = harmonic n := by
  induction n with
  | zero => rfl
  | succ m ih =>
      unfold harmonicRat at ih ⊢
      rw [Finset.sum_Icc_succ_top (Nat.le_add_left 1 m), ih, harmonic_succ]
      push_cast
      rw [one_div]

/-- Canonical bridge lemma matching `harmonicNum` with `(harmonic n : ℝ)`. -/
theorem bridge_harmonicNum (n : ℕ) :
    harmonicNum n = (harmonic n : ℝ) := by
  have hcast : harmonicNum n = (harmonicRat n : ℝ) := by
    unfold harmonicNum harmonicRat
    push_cast
    rfl
  rw [hcast, bridge_harmonicRat]
