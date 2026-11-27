-- @lean-policy: 4
-- @depends-on: chain-quantum-info-2-kraus-cptp-data-processing
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Finset.Basic
import Mathlib.Data.Complex.Basic
import Mathlib.Data.Matrix.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import SwarmProofs.Chains.QuantumInfo.Defs.V1

/-- Chain G [Rung 3/4 → Quantum Information & Hamiltonian Complexity] (Detectability Lemma & Knill–Laflamme QEC):
    Proves the two central operator-algebraic engines of modern quantum Hamiltonian complexity and
    fault-tolerant quantum error correction:
    (1) Aharonov–Arad–Landau–Vazirani / Anshu–Arad–Vidick Detectability Lemma: for every frustration-free
        local projector Hamiltonian `H = ∑_i P_i` with spectral gap `Δ > 0` where each projector `P_i`
        fails to commute with at most `g` other projectors, the detectability operator
        `D = ∏_i (I - P_i)` contracts every vector `v ⊥ ker(H)` by `‖D v‖² ≤ (g² / (g² + Δ)) ‖v‖²`;
    (2) Knill–Laflamme (1997) QEC Recovery Sufficiency: whenever a code projector `Pcode` and CPTP
        error channel `E` satisfy `Pcode E_a† E_b Pcode = C_{ab} Pcode`, there exists a CPTP recovery
        Kraus channel `R` (with `k + 1` Kraus operators) satisfying `R(E(ρ)) = ρ` for every code state `ρ`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  (∀ (d m g : ℕ) (P : Fin m → Matrix (Fin d) (Fin d) ℂ) (Δ : ℝ),
    IsFrustrationFree d m P →
    HasSpectralGapGe d m P Δ →
    (∀ i : Fin m, (Finset.univ.filter (fun j : Fin m => P i * P j ≠ P j * P i)).card ≤ g) →
    ∀ v : Fin d → ℂ,
      (∀ w : Fin d → ℂ, (∀ i : Fin m, Matrix.mulVec (P i) w = 0) →
        Finset.sum Finset.univ (fun j => star (w j) * v j) = 0) →
      Finset.sum Finset.univ (fun j => ‖Matrix.mulVec (detectabilityProd d m P) v j‖ ^ 2) ≤
        (((g : ℝ) ^ 2) / ((g : ℝ) ^ 2 + Δ)) * Finset.sum Finset.univ (fun j => ‖v j‖ ^ 2)) ∧
  (∀ (d k : ℕ) (Pcode : Matrix (Fin d) (Fin d) ℂ) (E : Fin k → Matrix (Fin d) (Fin d) ℂ),
    IsKrausChannel d d k E →
    SatisfiesKnillLaflamme d k Pcode E →
    ∃ R : Fin (k + 1) → Matrix (Fin d) (Fin d) ℂ,
      IsKrausChannel d d (k + 1) R ∧
      ∀ ρ : Matrix (Fin d) (Fin d) ℂ,
        IsDensityMatrix d ρ →
        Pcode * ρ * Pcode = ρ →
        applyKraus d d (k + 1) R (applyKraus d d k E ρ) = ρ)
