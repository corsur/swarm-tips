-- @lean-policy: 4
-- @depends-on: chain-rh-analytic-3-pnt-mertens-little-o
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import SwarmProofs.Chains.RHAnalytic.Defs

/-- Chain C [Rung 4/4 → RH Analytic Mertens Hierarchy] (Open Zero-Free Strip Re(s) > 0.99):
    Establishes a uniform power-saving Mertens bound `|M(N)| ≤ C^{1/100} · N^{99/100}` for all
    `N ≥ 1` (quantified with outer constant `∃ C ≥ 1`). Strictly implied by `millennium-rh-mertens-quasi`
    (`θ = 3/4`) and strictly implies Rung C3 (`PNT`), as kernel-verified in
    `proofs/millennium-chains/rh_analytic_chain.lean`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∃ (C : ℤ), 1 ≤ C ∧ ∀ (N : ℕ), 1 ≤ N → (mertensSum N) ^ 100 ≤ C * ((N : ℤ) ^ 99)
