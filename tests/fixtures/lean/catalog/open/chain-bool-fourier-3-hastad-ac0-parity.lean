-- @lean-policy: 4
-- @depends-on: chain-bool-fourier-2-kkl-influential-variable
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Analysis.SpecialFunctions.Pow.Real
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import SwarmProofs.Chains.BoolFourier.Defs.V1

/-- Chain F [Rung 3/4 → Boolean Fourier Analysis & Complexity] (Håstad 1986 Switching Lemma / PARITY ∉ AC⁰):
    Unconditionally provable landmark of constant-depth circuit complexity: there exists a universal
    constant `c > 0` such that for every depth `d ≥ 2`, for all sufficiently large `n ≥ N₀(d)`,
    every unbounded-fanin De Morgan (`AC^0`) formula `φ : AC0Formula n` of depth `≤ d` computing
    exact `n`-bit parity (`parityFin n`) has size at least `exp(c · n^{1 / (d - 1)})`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∃ (c : ℝ), 0 < c ∧
    ∀ (d : ℕ), 2 ≤ d →
      ∃ (N₀ : ℕ),
        ∀ (n : ℕ), N₀ ≤ n →
          ∀ (φ : AC0Formula n),
            ac0Depth φ ≤ d →
            (∀ x : Fin n → Bool, evalAC0 x φ = parityFin n x) →
            Real.exp (c * ((n : ℝ) ^ ((1 : ℝ) / ((d - 1 : ℕ) : ℝ)))) ≤ (ac0Size φ : ℝ)
