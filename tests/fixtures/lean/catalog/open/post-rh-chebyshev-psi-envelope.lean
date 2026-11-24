import Mathlib.NumberTheory.LSeries.RiemannZeta
import Mathlib.NumberTheory.ArithmeticFunction.VonMangoldt
import Mathlib.Analysis.SpecialFunctions.Pow.Real
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Order.Interval.Finset.Nat
import Mathlib.Algebra.Order.Floor.Ring

/-!
# Post-RH Consequences: Von Koch's Chebyshev `ψ(x)` Error Envelope (`RiemannHypothesis → ...`)

Helge von Koch (1901) proved that the Riemann Hypothesis implies the square-root Prime
Number Theorem error envelope for Chebyshev's second function $\psi(x) = \sum_{1 \le n \le x} \Lambda(n)$:
$$|\psi(x) - x| \le C \sqrt{x} (\log x)^2 \qquad \text{for all real } x \ge 2$$
for an absolute constant $C > 0$ (later sharpened explicitly by Schoenfeld, 1976).

Because the bounding constant `C > 0` is quantified outside `∀ x ≥ 2`, the statement
captures the true uniform analytic error envelope and connects Mathlib's `RiemannHypothesis`
directly to `ArithmeticFunction.vonMangoldt`.
-/

/-- Assuming Mathlib's `RiemannHypothesis`, there exists an absolute constant `C > 0` such that
    for every real `x ≥ 2`, `|∑_{n=1}^{⌊x⌋} Λ(n) - x| ≤ C * √x * (log x)^2`. -/
def statementProp : Prop :=
  RiemannHypothesis →
    ∃ (C : ℝ), 0 < C ∧
      ∀ (x : ℝ), 2 ≤ x →
        |Finset.sum (Finset.Icc 1 (Nat.floor x))
          (fun n => ArithmeticFunction.vonMangoldt n) - x| ≤
          C * Real.sqrt x * (Real.log x) ^ 2
