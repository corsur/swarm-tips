/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

import Mathlib.Data.Fin.Basic
import Mathlib.Data.Complex.Basic
import Mathlib.Data.Matrix.Basic
import Mathlib.Data.Matrix.Mul
import Mathlib.LinearAlgebra.Matrix.Trace
import Mathlib.LinearAlgebra.Matrix.PosDef
import Mathlib.Analysis.Matrix.Hermitian
import Mathlib.Analysis.Matrix.PosDef
import Mathlib.Analysis.Matrix.Spectrum
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

/-- Quadratic form `⟨v, A v⟩ = ∑_i conj(v_i) · (A v)_i` on `ℂ^d`. -/
def matQuadForm (d : ℕ) (A : Matrix (Fin d) (Fin d) ℂ) (v : Fin d → ℂ) : ℂ :=
  Finset.sum Finset.univ (fun i => star (v i) * Matrix.mulVec A v i)

/-- Positive semidefiniteness of a Hermitian matrix `A`: `∀ v ∈ ℂ^d, 0 ≤ Re⟨v, A v⟩`. -/
def IsHermitianPSD (d : ℕ) (A : Matrix (Fin d) (Fin d) ℂ) : Prop :=
  A.IsHermitian ∧ ∀ v : Fin d → ℂ, 0 ≤ (matQuadForm d A v).re

/-- A finite-dimensional quantum density matrix: Hermitian, positive semidefinite, unit trace. -/
def IsDensityMatrix (d : ℕ) (ρ : Matrix (Fin d) (Fin d) ℂ) : Prop :=
  IsHermitianPSD d ρ ∧ ρ.trace = 1

/-- A finite-dimensional binary quantum observable (`±1`-valued Hermitian involution `A² = I`). -/
def IsQuantumObservable (d : ℕ) (A : Matrix (Fin d) (Fin d) ℂ) : Prop :=
  A.IsHermitian ∧ A * A = 1

/-- Commuting Alice (`A0, A1`) and Bob (`B0, B1`) binary quantum observables for the CHSH game. -/
def CommutingCHSHObservables (d : ℕ)
    (A0 A1 B0 B1 : Matrix (Fin d) (Fin d) ℂ) : Prop :=
  IsQuantumObservable d A0 ∧
  IsQuantumObservable d A1 ∧
  IsQuantumObservable d B0 ∧
  IsQuantumObservable d B1 ∧
  A0 * B0 = B0 * A0 ∧
  A0 * B1 = B1 * A0 ∧
  A1 * B0 = B0 * A1 ∧
  A1 * B1 = B1 * A1

/-- Quantum CHSH Bell operator `B = A0 B0 + A0 B1 + A1 B0 - A1 B1`. -/
def chshOperator (d : ℕ)
    (A0 A1 B0 B1 : Matrix (Fin d) (Fin d) ℂ) : Matrix (Fin d) (Fin d) ℂ :=
  A0 * B0 + A0 * B1 + A1 * B0 - A1 * B1

/-- Real expectation value `Re(Tr(ρ · B))` of a CHSH operator on state `ρ`. -/
def chshExpectation (d : ℕ)
    (ρ A0 A1 B0 B1 : Matrix (Fin d) (Fin d) ℂ) : ℝ :=
  ((ρ * chshOperator d A0 A1 B0 B1).trace).re

/-- Classical deterministic CHSH score `a0 b0 + a0 b1 + a1 b0 - a1 b1 ∈ {-2, +2}`
    for binary strategies `a0, a1, b0, b1 ∈ {-1, +1}` encoded as `Bool`. -/
def signInt (b : Bool) : ℤ :=
  if b then -1 else 1

def classicalChshValue (a0 a1 b0 b1 : Bool) : ℤ :=
  signInt a0 * signInt b0 +
  signInt a0 * signInt b1 +
  signInt a1 * signInt b0 -
  signInt a1 * signInt b1

/-- A completely positive trace-preserving (CPTP) quantum channel represented by `m` Kraus
    operators `K : Fin m → Matrix (Fin dOut) (Fin dIn) ℂ` satisfying `∑_a K_a† K_a = I`. -/
def IsKrausChannel (dIn dOut m : ℕ)
    (K : Fin m → Matrix (Fin dOut) (Fin dIn) ℂ) : Prop :=
  (Finset.sum Finset.univ (fun a => (K a).conjTranspose * K a)) = 1

/-- Action of a Kraus channel `E(ρ) = ∑_a K_a ρ K_a†`. -/
def applyKraus (dIn dOut m : ℕ)
    (K : Fin m → Matrix (Fin dOut) (Fin dIn) ℂ)
    (ρ : Matrix (Fin dIn) (Fin dIn) ℂ) : Matrix (Fin dOut) (Fin dOut) ℂ :=
  Finset.sum Finset.univ (fun a => K a * ρ * (K a).conjTranspose)

/-- Binary scalar `x log x` convention (`0 ↦ 0`) used for von Neumann entropy. -/
noncomputable def xLogX (x : ℝ) : ℝ :=
  if x ≤ 0 then 0 else x * Real.log x

/-- Von Neumann entropy `S(ρ) = -∑_i λ_i log(λ_i)` of a Hermitian matrix `ρ` via its
    spectral eigenvalues (`hρ : ρ.IsHermitian`). -/
noncomputable def vonNeumannEntropy (d : ℕ) (ρ : Matrix (Fin d) (Fin d) ℂ)
    (hρ : ρ.IsHermitian) : ℝ :=
  - Finset.sum Finset.univ (fun i : Fin d => xLogX (hρ.eigenvalues i))

/-- Operator-norm squared upper bound `‖M v‖₂² ≤ c · ‖v‖₂²` for all `v ∈ ℂ^d`. -/
def OpNormSqLe (d : ℕ) (M : Matrix (Fin d) (Fin d) ℂ) (c : ℝ) : Prop :=
  ∀ v : Fin d → ℂ,
    Finset.sum Finset.univ (fun i => ‖(Matrix.mulVec M v) i‖ ^ 2) ≤
      c * Finset.sum Finset.univ (fun i => ‖v i‖ ^ 2)

/-- A family of `m` local orthogonal projectors `P_1, ..., P_m` on `ℂ^d` forming a
    local projector Hamiltonian `H = ∑_i P_i` (setting of the Detectability Lemma and QMA/NLTS). -/
def IsProjectorFamily (d m : ℕ) (P : Fin m → Matrix (Fin d) (Fin d) ℂ) : Prop :=
  ∀ i : Fin m, (P i).IsHermitian ∧ P i * P i = P i

/-- Local Hamiltonian `H = ∑_i P_i` associated to a family of local projectors. -/
def localProjectorHam (d m : ℕ) (P : Fin m → Matrix (Fin d) (Fin d) ℂ) :
    Matrix (Fin d) (Fin d) ℂ :=
  Finset.sum Finset.univ P

/-- Frustration-free local projector Hamiltonian: admits a nonzero zero-energy ground state. -/
def IsFrustrationFree (d m : ℕ) (P : Fin m → Matrix (Fin d) (Fin d) ℂ) : Prop :=
  IsProjectorFamily d m P ∧
  ∃ v : Fin d → ℂ, v ≠ 0 ∧ ∀ i : Fin m, Matrix.mulVec (P i) v = 0

/-- Detectability operator `D = ∏_{i=0}^{m-1} (I - P_i)` obtained by multiplying the
    complementary projectors `(I - P_i)` in order. -/
def detectabilityProd (d m : ℕ) (P : Fin m → Matrix (Fin d) (Fin d) ℂ) :
    Matrix (Fin d) (Fin d) ℂ :=
  (List.finRange m).foldl (fun acc i => acc * (1 - P i)) 1

/-- Quantum error-correcting code subspace (Knill–Laflamme setting): an orthogonal
    projector `Pcode` onto the code space and a family of Kraus error operators `E_a`. -/
def SatisfiesKnillLaflamme (d k : ℕ) (Pcode : Matrix (Fin d) (Fin d) ℂ)
    (E : Fin k → Matrix (Fin d) (Fin d) ℂ) : Prop :=
  Pcode.IsHermitian ∧ Pcode * Pcode = Pcode ∧
  ∃ C : Fin k → Fin k → ℂ,
    ∀ a b : Fin k, Pcode * (E a).conjTranspose * E b * Pcode = (C a b) • Pcode

/-- Spectral gap lower bound `Δ > 0` for a frustration-free Hamiltonian `H`:
    for every `v` orthogonal to `ker(H)`, `⟨v, H v⟩ ≥ Δ ‖v‖²`. -/
def HasSpectralGapGe (d m : ℕ) (P : Fin m → Matrix (Fin d) (Fin d) ℂ) (Δ : ℝ) : Prop :=
  0 < Δ ∧
  ∀ v : Fin d → ℂ,
    (∀ w : Fin d → ℂ, (∀ i : Fin m, Matrix.mulVec (P i) w = 0) →
      Finset.sum Finset.univ (fun j => star (w j) * v j) = 0) →
    Δ * Finset.sum Finset.univ (fun j => ‖v j‖ ^ 2) ≤
      (matQuadForm d (localProjectorHam d m P) v).re

#guard classicalChshValue false false false false = 2
#guard classicalChshValue false true false true = -2
#guard classicalChshValue true false true false = -2
#guard (List.all [false, true] fun a0 =>
  List.all [false, true] fun a1 =>
  List.all [false, true] fun b0 =>
  List.all [false, true] fun b1 =>
    decide (Int.natAbs (classicalChshValue a0 a1 b0 b1) ≤ 2)) = true
