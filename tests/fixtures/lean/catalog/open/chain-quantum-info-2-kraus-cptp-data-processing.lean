-- @lean-policy: 4
-- @depends-on: chain-quantum-info-1-chsh-tsirelson-bound
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Complex.Basic
import Mathlib.Data.Matrix.Basic
import Mathlib.Analysis.Matrix.Hermitian
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import SwarmProofs.Chains.QuantumInfo.Defs.V1

/-- Every CPTP Kraus channel maps every finite-dimensional density matrix to a density matrix. -/
def KrausPreservesDensity : Prop :=
  ∀ (dIn dOut m : ℕ)
      (K : Fin m → Matrix (Fin dOut) (Fin dIn) ℂ)
      (ρ : Matrix (Fin dIn) (Fin dIn) ℂ),
    IsKrausChannel dIn dOut m K →
    IsDensityMatrix dIn ρ →
    IsDensityMatrix dOut (applyKraus dIn dOut m K ρ)

/-- Fundamental von Neumann entropy bounds: `0 ≤ S(ρ) ≤ log d` for every `d ≥ 1` density matrix `ρ`. -/
def VonNeumannEntropyBounds : Prop :=
  ∀ (d : ℕ), 1 ≤ d →
    ∀ (ρ : Matrix (Fin d) (Fin d) ℂ) (hρ : IsDensityMatrix d ρ),
      0 ≤ vonNeumannEntropy d ρ hρ.1.1 ∧
      vonNeumannEntropy d ρ hρ.1.1 ≤ Real.log (d : ℝ)

/-- Uhlmann's Quantum Second Law: every unital CPTP Kraus channel (`E(I) = I`) never decreases
    von Neumann entropy (`S(ρ) ≤ S(E(ρ))`). -/
def UnitalKrausEntropyMonotone : Prop :=
  ∀ (d m : ℕ)
      (K : Fin m → Matrix (Fin d) (Fin d) ℂ)
      (ρ : Matrix (Fin d) (Fin d) ℂ),
    IsKrausChannel d d m K →
    applyKraus d d m K (1 : Matrix (Fin d) (Fin d) ℂ) = 1 →
    ∀ (hρ : IsDensityMatrix d ρ)
      (hE : (applyKraus d d m K ρ).IsHermitian),
      vonNeumannEntropy d ρ hρ.1.1 ≤ vonNeumannEntropy d (applyKraus d d m K ρ) hE

/-- Chain G [Rung 2/4 → Quantum Information & Hamiltonian Complexity] (Kraus CPTP Channels & Quantum Entropy Law):
    Establishes the core mathematical pillars of finite-dimensional quantum channels and von Neumann entropy:
    (1) CPTP state preservation: every Kraus channel `K` maps every density matrix `ρ` to a density matrix;
    (2) Fundamental von Neumann entropy bounds: for every `d ≥ 1` and density matrix `ρ`,
        `0 ≤ S(ρ) ≤ log d`;
    (3) Uhlmann's Quantum Second Law (unital data processing / bistochastic majorization):
        every unital Kraus channel (`E(I) = I`) never decreases von Neumann entropy, `S(ρ) ≤ S(E(ρ))`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  KrausPreservesDensity ∧ VonNeumannEntropyBounds ∧ UnitalKrausEntropyMonotone
