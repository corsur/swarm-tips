-- @lean-policy: 4
-- @depends-on: chain-rh-lagarias-2-normalized-monotonicity
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import SwarmProofs.Chains.RHLagarias.Defs

/-- Chain A [Rung 3/4 → RH Lagarias Equivalence] (Choie–Lichiardopol–Moree–Solé 2007):
    Lagarias's Riemann Hypothesis inequality holds unconditionally for every odd positive
    integer and every squarefree positive integer, eliminating both infinite families from
    the open Millennium target `millennium-rh-lagarias-divisor`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 1 ≤ n → (n % 2 = 1 ∨ isSquarefreePos n) → lagariasBound n
