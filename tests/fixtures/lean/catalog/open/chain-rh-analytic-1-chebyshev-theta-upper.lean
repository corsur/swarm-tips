import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

open scoped BigOperators

/-- Chain C [Rung 1/4 → RH Analytic Mertens Hierarchy] (Chebyshev–Erdős Prime Product Bound):
    For every integer `N ≥ 1`, the primorial product `∏_{p ≤ N, p prime} p` is bounded above by
    `4 ^ N` (equivalently `ϑ(N) = ∑_{p ≤ N} log p ≤ (2 log 2) · N`), proved classically by
    comparing prime-factor divisibility of central binomial coefficients `C(2m+1, m) < 4^m`. -/
def statementProp : Prop :=
  ∀ (N : ℕ), 1 ≤ N →
    (∏ p ∈ (Finset.range (N + 1)).filter Nat.Prime, p) ≤ 4 ^ N
