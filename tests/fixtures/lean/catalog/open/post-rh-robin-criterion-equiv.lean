import Mathlib.NumberTheory.LSeries.RiemannZeta
import Mathlib.NumberTheory.Harmonic.EulerMascheroni
import Mathlib.NumberTheory.ArithmeticFunction.Misc
import Mathlib.Analysis.SpecialFunctions.Log.Basic

/-!
# Post-RH Consequences: Robin's Divisor-Sum Inequality (`RiemannHypothesis → ...`)

Guy Robin (1984) proved that the Riemann Hypothesis is equivalent to the elementary
divisor-sum inequality
$$\sigma(n) < e^{\gamma} n \log \log n \qquad \text{for all } n > 5040,$$
where $\sigma(n) = \sum_{d \mid n} d$ and $\gamma$ is the Euler–Mascheroni constant
(`Real.eulerMascheroniConstant`). The threshold $n > 5040$ is sharp: $n = 5040$ satisfies
$\sigma(5040) = 19344 > e^\gamma \cdot 5040 \log \log 5040 \approx 19237.2$.

In the post-RH landscape, formalizing classical conditional consequences directly from
Mathlib's canonical `RiemannHypothesis` predicate unlocks reusable analytic number theory
bridges without waiting for the full proof of RH to be upstreamed into Mathlib.
-/

/-- Assuming Mathlib's `RiemannHypothesis`, Robin's inequality holds for every integer
    `n > 5040`: `σ₁(n) < exp(γ) * n * log (log n)`. -/
def statementProp : Prop :=
  RiemannHypothesis →
    ∀ (n : ℕ), 5040 < n →
      (ArithmeticFunction.sigma 1 n : ℝ) <
        Real.exp Real.eulerMascheroniConstant * (n : ℝ) * Real.log (Real.log (n : ℝ))
