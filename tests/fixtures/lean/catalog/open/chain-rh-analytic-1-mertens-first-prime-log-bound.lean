-- @lean-policy: 4
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import SwarmProofs.Chains.RHAnalytic.Defs.V1

/-- Chain C [Rung 1/4 → RH Analytic Mertens Hierarchy] (Mertens' First Theorem, 1874):
    There exists an absolute constant `C ≥ 0` such that for every integer `N ≥ 2`,
    the prime-weighted logarithmic sum `∑_{p ≤ N, p prime} (log p) / p` stays within `C`
    of `log N`:
    `|∑_{p ≤ N, p prime} (log p) / p - log N| ≤ C`,
    proved classically by comparing Legendre's prime-power factorization of `N!`
    (`log(N!) = N log N - N + O(log N)`) with Chebyshev's upper bound `ϑ(N) ≤ N log 4`
    and bounding proper prime-power contributions `∑_{p, k ≥ 2} (log p) / p^k = O(1)`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∃ (C : ℝ), 0 ≤ C ∧
    ∀ (N : ℕ), 2 ≤ N →
      |Finset.sum ((Finset.range (N + 1)).filter Nat.Prime)
        (fun p => Real.log (p : ℝ) / (p : ℝ)) - Real.log (N : ℝ)| ≤ C
