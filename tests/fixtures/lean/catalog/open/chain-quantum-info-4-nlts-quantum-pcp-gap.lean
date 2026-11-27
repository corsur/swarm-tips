-- @lean-policy: 4
-- @depends-on: chain-quantum-info-3-detectability-lemma-knill-laflamme
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Finset.Basic
import Mathlib.Data.Complex.Basic
import Mathlib.Data.Matrix.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import SwarmProofs.Chains.QuantumInfo.Defs.V1

/-- Bit `j` (`0` or `1`) of computational-basis index `x : Fin (2^n)`. -/
def qubitBit (n : ℕ) (x : Fin (2 ^ n)) (j : Fin n) : ℕ :=
  (x.val / (2 ^ j.val)) % 2

/-- Two-qubit computational-basis index `(x_a, x_b) ∈ Fin 4` extracted from `x : Fin (2^n)`. -/
def twoQubitIdx (n : ℕ) (a b : Fin n) (x : Fin (2 ^ n)) : Fin 4 :=
  ⟨2 * qubitBit n x a + qubitBit n x b, by
    have ha : qubitBit n x a < 2 := Nat.mod_lt _ (by decide)
    have hb : qubitBit n x b < 2 := Nat.mod_lt _ (by decide)
    omega⟩

/-- Lift a `4 × 4` two-qubit gate `U` acting on qubits `(a, b)` to an `n`-qubit operator on `ℂ^{2^n}`. -/
def lift2Qubit (n : ℕ) (a b : Fin n) (U : Matrix (Fin 4) (Fin 4) ℂ) :
    Matrix (Fin (2 ^ n)) (Fin (2 ^ n)) ℂ :=
  fun x y =>
    if (List.finRange n).all (fun j => j == a || j == b || qubitBit n x j == qubitBit n y j) then
      U (twoQubitIdx n a b x) (twoQubitIdx n a b y)
    else
      0

/-- A valid single parallel layer of disjoint `4 × 4` two-qubit unitary gates on `n` qubits. -/
def IsValid2QubitLayer (n : ℕ) (L : List (Fin n × Fin n × Matrix (Fin 4) (Fin 4) ℂ)) : Prop :=
  (L.flatMap (fun g => [g.1, g.2.1])).Nodup ∧
  ∀ g ∈ L, g.1 ≠ g.2.1 ∧ g.2.2.conjTranspose * g.2.2 = 1

/-- Unitary matrix of a single parallel 2-qubit gate layer. -/
def eval2QubitLayer (n : ℕ) (L : List (Fin n × Fin n × Matrix (Fin 4) (Fin 4) ℂ)) :
    Matrix (Fin (2 ^ n)) (Fin (2 ^ n)) ℂ :=
  L.foldl (fun acc g => acc * lift2Qubit n g.1 g.2.1 g.2.2) 1

/-- Computational basis product state `|0^n⟩ ∈ ℂ^{2^n}`. -/
def basisZeroState (n : ℕ) : Fin (2 ^ n) → ℂ :=
  fun x => if x.val = 0 then 1 else 0

/-- Output state `U_D ⋯ U_1 |0^n⟩` of a depth-`circ.length` all-to-all 2-qubit quantum circuit. -/
def evalCircuitState (n : ℕ)
    (circ : List (List (Fin n × Fin n × Matrix (Fin 4) (Fin 4) ℂ))) : Fin (2 ^ n) → ℂ :=
  circ.foldl (fun ψ L => Matrix.mulVec (eval2QubitLayer n L) ψ) (basisZeroState n)

/-- Exact characterization of a `q`-local operator `P = P_S ⊗ I_{S^c}` acting nontrivially only
    on a qubit subset `S` with `S.card ≤ q`. -/
def IsQLocalOperator (n q : ℕ) (P : Matrix (Fin (2 ^ n)) (Fin (2 ^ n)) ℂ) : Prop :=
  ∃ S : Finset (Fin n), S.card ≤ q ∧
    (∀ x y : Fin (2 ^ n), (∃ j : Fin n, j ∉ S ∧ qubitBit n x j ≠ qubitBit n y j) → P x y = 0) ∧
    (∀ x y x' y' : Fin (2 ^ n),
      (∀ j ∈ S, qubitBit n x j = qubitBit n x' j ∧ qubitBit n y j = qubitBit n y' j) →
      (∀ j : Fin n, j ∉ S → qubitBit n x j = qubitBit n y j ∧ qubitBit n x' j = qubitBit n y' j) →
      P x y = P x' y')

/-- Chain G [Rung 4/4 → Frontier: NLTS Theorem (Freedman–Hastings 2013 / Anshu–Breuckmann–Nirkhe 2022)]:
    The landmark Quantum PCP prerequisite (No Low-Energy Trivial States): there exist universal
    constants `q, g : ℕ` (locality and interaction degree), `ε > 0` (energy density), and `c > 0`
    such that for arbitrarily large qubit counts `n`, there exists a frustration-free `q`-local
    projector Hamiltonian `H = ∑_{i < m} P_i` (`m ≥ 1`, interaction degree `≤ g`) on `n` qubits
    whose every low-energy circuit state `ψ` (`Re⟨ψ, H ψ⟩ ≤ ε · m`) requires all-to-all 2-qubit
    quantum circuit depth at least `c · log n`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∃ (q g : ℕ) (ε c : ℝ), 0 < ε ∧ 0 < c ∧
    ∀ (N₀ : ℕ), ∃ (n m : ℕ), max N₀ 2 ≤ n ∧ 1 ≤ m ∧
      ∃ (P : Fin m → Matrix (Fin (2 ^ n)) (Fin (2 ^ n)) ℂ),
        IsFrustrationFree (2 ^ n) m P ∧
        (∀ i : Fin m, IsQLocalOperator n q (P i)) ∧
        (∀ i : Fin m, (Finset.univ.filter (fun j : Fin m => P i * P j ≠ P j * P i)).card ≤ g) ∧
        ∀ (circ : List (List (Fin n × Fin n × Matrix (Fin 4) (Fin 4) ℂ))),
          (∀ L ∈ circ, IsValid2QubitLayer n L) →
          (matQuadForm (2 ^ n) (localProjectorHam (2 ^ n) m P) (evalCircuitState n circ)).re ≤
            ε * (m : ℝ) →
          c * Real.log (n : ℝ) ≤ (circ.length : ℝ)
