import Mathlib.Data.Real.Basic
import Mathlib.Tactic.Ring

/-- Tier-2 Compositional Lemma 2 (N = 2 Base Case of Open Problem 1 `weil-gram-cubic-correlation`):
    For any 2×2 hollow symmetric matrix `G = [[0, a], [a, 0]]`, the diagonal of `G²` is `[a², a²]`
    with off-diagonal `0`, so the diagonal of `G³ = G² · G` vanishes identically and
    `tr(G³) = (a² * 0 + 0 * a) + (0 * a + a² * 0) = 0`. -/
def statementProp : Prop :=
  ∀ (a : ℝ),
    let g2_00 := 0 * 0 + a * a
    let g2_01 := 0 * a + a * 0
    let g2_10 := a * 0 + 0 * a
    let g2_11 := a * a + 0 * 0
    let g3_00 := g2_00 * 0 + g2_01 * a
    let g3_11 := g2_10 * a + g2_11 * 0
    g3_00 + g3_11 = 0
