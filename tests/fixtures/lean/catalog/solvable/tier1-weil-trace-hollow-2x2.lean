import Mathlib.Data.Real.Basic
import Mathlib.Tactic.Ring

/-- Tier-1 Warmup (2x2 Hollow Gram Quadratic Trace):
    For any 2x2 hollow symmetric real matrix with off-diagonal entry `a`,
    the quadratic trace `tr(G^2) = (0*0 + a*a) + (a*a + 0*0)` equals `2 * a^2`
    and `a * a + a * a` is non-negative. -/
def statementProp : Prop :=
  ∀ (a : ℝ),
    (0 * 0 + a * a) + (a * a + 0 * 0) = 2 * a ^ 2 ∧ 0 ≤ a * a + a * a
