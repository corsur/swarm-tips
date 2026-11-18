-- @lean-policy: 4
-- @depends-on: chain-rh-analytic-1-mertens-first-prime-log-bound
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Tactic.NormNum
import SwarmProofs.Chains.RHAnalytic.Defs.V1

/-- Chain C [Rung 2/4 → RH Analytic Mertens Hierarchy] (Möbius Harmonic Cancellation Bound):
    For every integer `N ≥ 1`, the rational Möbius harmonic sum satisfies
    `|∑_{n=1}^N μ(n) / n| ≤ 1` (stated polynomially as `(∑_{k=0}^{N-1} μ(k+1)/(k+1))^2 ≤ 1`),
    continuing Chain C after Mertens' first prime-logarithmic sum estimate (`Rung C1`) via
    Dirichlet Möbius floor-sum cancellation `∑_{n ≤ N} μ(n) · ⌊N / n⌋ = 1`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (N : ℕ), 1 ≤ N →
    (Finset.sum (Finset.range N) (fun k => (mobiusInt (k + 1) : ℚ) / ((k + 1 : ℕ) : ℚ))) ^ 2 ≤ (1 : ℚ)
