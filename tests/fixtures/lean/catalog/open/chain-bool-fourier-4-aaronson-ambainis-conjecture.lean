-- @lean-policy: 4
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Finset.Basic
import Mathlib.Data.Fintype.BigOperators
import Mathlib.Data.Fintype.Powerset
import Mathlib.Data.Real.Basic
import SwarmProofs.Chains.BoolFourier.Defs.V1

/-- Chain F [Rung 4/4 → Open Frontier: Aaronson–Ambainis Conjecture (2009)]:
    One of the premier open conjectures in Boolean Fourier analysis and quantum query complexity:
    there exist absolute constants `c > 0` and `k ≥ 1` such that every bounded real polynomial
    `f : (Fin n → Bool) → ℝ` (`|f x| ≤ 1`) of Fourier degree `≤ d` (`d ≥ 1`, `n ≥ 1`) possesses a
    coordinate `i : Fin n` with Fourier influence `∑_{S ∋ i} f̂(S)^2 ≥ c · (Var(f) / d)^k`
    (improving the Dinur–Friedgut–Kindler–O'Donnell exponential `4^{-d}` bound to `poly(Var(f)/d)`).
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∃ (c : ℝ) (k : ℕ), 0 < c ∧ 1 ≤ k ∧
    ∀ (n d : ℕ), 1 ≤ n → 1 ≤ d →
      ∀ (f : (Fin n → Bool) → ℝ),
        (∀ x : Fin n → Bool, |f x| ≤ 1) →
        (∀ S : Finset (Fin n), d < S.card → walshFourierCoeff n f S = 0) →
        ∃ i : Fin n,
          c * (boolVariance n f / (d : ℝ)) ^ k ≤
            Finset.sum (Finset.univ.filter (fun S : Finset (Fin n) => i ∈ S))
              (fun S => (walshFourierCoeff n f S) ^ 2)
