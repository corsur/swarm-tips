-- @lean-policy: 4
-- @depends-on: chain-bool-fourier-1-parseval-poincare-bonami
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import SwarmProofs.Chains.BoolFourier.Defs.V1

/-- Chain F [Rung 2/4 → Boolean Fourier Analysis & Complexity] (Kahn–Kalai–Linial 1988 Theorem):
    Unconditionally provable landmark via Bonami `(2,4)` hypercontractivity (Rung 1): there exists
    a universal constant `c > 0` such that for every `n ≥ 2` and every Boolean function
    `f : (Fin n → Bool) → Bool`, some coordinate `i : Fin n` has influence at least
    `c · Var(boolSign ∘ f) · (log n) / n` (matching the Ben-Or–Linial Tribes upper bound up to `c`).
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∃ (c : ℝ), 0 < c ∧
    ∀ (n : ℕ), 2 ≤ n →
      ∀ (f : (Fin n → Bool) → Bool),
        ∃ i : Fin n,
          c * boolVariance n (fun x => boolSign (f x)) * (Real.log (n : ℝ) / (n : ℝ)) ≤
            coordInfluence n f i
