-- @lean-policy: 4
-- @depends-on: chain-rh-lagarias-3-odd-and-squarefree
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import SwarmProofs.Chains.RHLagarias.Defs

/-- Chain A [Rung 4/4 → RH Lagarias Equivalence] (Narrowed Even Non-Squarefree Superabundant Core):
    Restricts Lagarias's RH equivalence to the finite base `[1, 5040]` plus even, non-squarefree
    superabundant integers `n > 5040` (Akbary–Friggstad 2009). Combined with Rungs A1–A3 via
    `proofs/millennium-chains/rh_lagarias_chain.lean`, this implies full RH.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  (∀ (n : ℕ), 1 ≤ n → n ≤ 5040 → lagariasBound n) ∧
    (∀ (n : ℕ), 5040 < n → isSuperabundant n → n % 2 = 0 → ¬ isSquarefreePos n → lagariasBound n)
