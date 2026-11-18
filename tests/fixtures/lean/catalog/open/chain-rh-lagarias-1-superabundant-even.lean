-- @lean-policy: 4
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import SwarmProofs.Chains.RHLagarias.Defs

/-- Chain A [Rung 1/4 → RH Lagarias Equivalence] (Alaoglu–Erdős Parity Lemma, 1944):
    Proves the base case `lagariasBound 1` together with the structural parity theorem that
    every superabundant integer `n ≥ 2` is even (`n % 2 = 0`), by swapping an odd prime power
    `p^a` for `2^a` in the multiplicative divisor-sum formula.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  lagariasBound 1 ∧ ∀ (n : ℕ), 2 ≤ n → isSuperabundant n → n % 2 = 0
