-- @lean-policy: 4
-- @depends-on: chain-rh-analytic-2-mobius-harmonic-bound
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Tactic.Linarith
import SwarmProofs.Chains.RHAnalytic.Defs.V1

/-- Chain C [Rung 3/4 → RH Analytic Mertens Hierarchy] (Landau 1899 PNT Equivalence):
    Sublinear cancellation `M(N) = o(N)` of the Mertens function (`∀ k ≥ 1, ∃ N₀, ∀ N ≥ N₀,
    k · |M(N)| ≤ N`), strictly equivalent to the Prime Number Theorem and non-vanishing of
    `ζ(1 + it)` on the vertical boundary `Re(s) = 1`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (k : ℕ), 1 ≤ k → ∃ (N₀ : ℕ), ∀ (N : ℕ), N₀ ≤ N → (k : ℤ) * |mertensSum N| ≤ (N : ℤ)
