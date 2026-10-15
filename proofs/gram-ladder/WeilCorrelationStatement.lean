/- SPDX-License-Identifier: Apache-2.0 -/
import Mathlib
import Zeta23

open scoped BigOperators Matrix

/-- Weil Gram matrix higher-degree moments at critical scale λ = 1.
    For the Gram matrix G of the Weil explicit formula with bandwidth parameter
    P.lam = 1 (matching local mean zero spacing 2π/log T), the fourth trace
    moment tr(G⁴) expands into quadruple products of prime logarithms
    ∑_{p₁,p₂,p₃,p₄} Λ(p₁)Λ(p₂)Λ(p₃)Λ(p₄)/(p₁p₂p₃p₄)^½.

    This statement formalizes the uniform open bound (∃ C ≥ 0 outside ∀ P, T)
    on the off-diagonal quadruple correlation, asserting that tr(G⁴) satisfies
    the GUE Gaussian-pairing asymptotic on par with Montgomery's 4-point
    correlation conjecture. -/
def statementProp : Prop :=
  ∃ (C : ℝ), 0 ≤ C ∧
    ∀ (P : Zeta23.Defs.Params) (_hValid : P.Valid) (_hLam : P.lam = 1)
      (T : ℝ) (_hT : P.T0 ≤ T),
      let n := P.M + 1
      let G : Matrix (Fin n) (Fin n) ℝ := fun i j => P.Gentry T i.val j.val
      |Matrix.trace (G ^ 4) - 2 * (Matrix.trace (G ^ 2)) ^ 2| ≤
        C * Matrix.trace (G ^ 2)
