-- @lean-policy: 4
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Complex.Basic
import Mathlib.Data.Matrix.Basic
import Mathlib.Analysis.Real.Sqrt
import SwarmProofs.Chains.QuantumInfo.Defs.V1

/-- Chain G [Rung 1/4 → Quantum Information & Hamiltonian Complexity] (Bell / CHSH & Exact Tsirelson Bound):
    Proves both directions of Tsirelson's (1980) foundational theorem separating quantum entanglement
    from classical local hidden-variable correlations (`|CHSH_classical| ≤ 2`):
    (1) Universal quantum ceiling: for EVERY dimension `d : ℕ`, density matrix `ρ`, and commuting
        Alice/Bob binary observables `A0, A1, B0, B1`, `|chshExpectation d ρ A0 A1 B0 B1| ≤ 2 √2`;
    (2) Exact saturation in `d = 4` (2 qubits, Bell state `|Φ^+⟩`): there exist a density matrix
        `ρ` and commuting binary observables achieving `chshExpectation 4 ρ A0 A1 B0 B1 = 2 √2`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  (∀ (d : ℕ) (ρ A0 A1 B0 B1 : Matrix (Fin d) (Fin d) ℂ),
    IsDensityMatrix d ρ →
    CommutingCHSHObservables d A0 A1 B0 B1 →
    |chshExpectation d ρ A0 A1 B0 B1| ≤ 2 * Real.sqrt 2) ∧
  (∃ (ρ A0 A1 B0 B1 : Matrix (Fin 4) (Fin 4) ℂ),
    IsDensityMatrix 4 ρ ∧
    CommutingCHSHObservables 4 A0 A1 B0 B1 ∧
    chshExpectation 4 ρ A0 A1 B0 B1 = 2 * Real.sqrt 2)
