import Mathlib.LinearAlgebra.Matrix.Trace
import Mathlib.LinearAlgebra.Matrix.Symmetric
import Mathlib.Data.Real.Basic

open scoped BigOperators Matrix

/-- Open problem in analytic number theory: bounding the quadruple correlation of prime
    logarithms via the fourth trace moment tr(G⁴) of the Weil explicit formula Gram matrix
    at critical zero-spacing scale λ = 1 (on par with Montgomery-Odlyzko 4-point correlation). -/
def statementProp : Prop :=
  ∀ (n : ℕ) (G : Matrix (Fin n) (Fin n) ℝ),
    G.IsSymm →
    (∀ (v : Fin n → ℝ), 0 ≤ ∑ i : Fin n, ∑ j : Fin n, v i * G i j * v j) →
    ∃ (C : ℝ), 0 ≤ C ∧
      |Matrix.trace (G ^ 4) - 2 * (Matrix.trace (G ^ 2)) ^ 2| ≤ C * Matrix.trace (G ^ 2)
