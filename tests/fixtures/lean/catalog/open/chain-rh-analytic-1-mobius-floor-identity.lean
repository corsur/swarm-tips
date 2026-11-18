-- @lean-policy: 4
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import SwarmProofs.Chains.RHAnalytic.Defs

/-- Chain C [Rung 1/4 → RH Analytic Mertens Hierarchy] (Meissel's Möbius Floor-Sum Identity, 1854):
    For every integer `N ≥ 1`, the Möbius floor sum satisfies
    `∑_{k=1}^N μ(k) · ⌊N / k⌋ = 1` (written over `0 ≤ k < N` as `mobiusInt (k + 1) * ⌊N / (k + 1)⌋`),
    derived by grouping divisor pairs `d · q ≤ N` via Dirichlet Möbius inversion `∑_{d ∣ m} μ(d) = [m = 1]`.
    Rung C2 (`chain-rh-analytic-2-mobius-harmonic-bound`) builds directly on this identity.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (N : ℕ), 1 ≤ N →
    Finset.sum (Finset.range N) (fun k => mobiusInt (k + 1) * ((N / (k + 1) : ℕ) : ℤ)) = 1
