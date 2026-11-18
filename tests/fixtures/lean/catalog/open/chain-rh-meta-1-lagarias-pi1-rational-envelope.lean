-- @lean-policy: 4
import Mathlib.Data.Nat.Factorial.Basic
import Mathlib.Data.Rat.Cast.Order
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import SwarmProofs.Chains.RHLagarias.Defs

/-- Track E [Rung 1/4 → Π₁⁰ RH Metamathematics] (Rational Π₁⁰ Envelope Equivalence):
    Eliminates ℝ, Real.exp, and Real.log from Lagarias's RH criterion without Schanuel's
    Conjecture by proving that for every `n ≥ 1`, `lagariasBound n` holds iff the purely
    decidable rational inequality `sigmaRat n ≤ lagariasEnvelopeRat n K` holds for all `K ≥ 1`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 1 ≤ n →
    (lagariasBound n ↔ ∀ (K : ℕ), 1 ≤ K → sigmaRat n ≤ lagariasEnvelopeRat n K)
