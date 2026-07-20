import Lproofs.Schemes.Fold

/-! @lc 3371 | name:Identify the Largest Outlier in an Array | scheme:fold | family:hashing |
    complexity:O(n) | source:https://leetcode.com/problems/identify-the-largest-outlier-in-an-array/

    The array holds some "special" numbers, one element equal to their sum, and one "outlier"; the total
    is therefore `2·(special sum) + outlier`. CLASSIFICATION (fold): the total is a streaming sum fold.
    CORRECTNESS: we certify the arithmetic identity the solution inverts — for any split into specials,
    their sum-element, and an outlier, `outlier = total − 2·(special sum)` — so scanning candidate
    sum-elements recovers the outlier exactly. -/

namespace LC.P3371
open Interview.Patterns

/-- The full array: the specials, their sum-element, and the outlier. -/
def sol (specials : List ℤ) (outlier : ℤ) : ℤ := (specials ++ [specials.sum, outlier]).sum

/-- SCHEME (fold): the array total is a streaming right-fold sum — and `sol` is that sum over
    the specials, their sum-element, and the outlier. -/
theorem cls : (IsRightFold (List.sum : List ℤ → ℤ)) ∧
    ∀ (specials : List ℤ) (outlier : ℤ),
      sol specials outlier = (specials ++ [specials.sum, outlier]).sum := by
  refine ⟨⟨(· + ·), 0, fun L => ?_⟩, fun _ _ => rfl⟩
  induction L with
  | nil => rfl
  | cons a t ih => simp [List.sum_cons, ih]

/-- CORRECT: the outlier equals the total minus twice the special sum — the identity that lets a single
    pass recover the outlier from any candidate sum-element. -/
theorem corr (specials : List ℤ) (outlier : ℤ) :
    outlier = sol specials outlier - 2 * specials.sum := by
  simp only [sol, List.sum_append, List.sum_cons, List.sum_nil]
  ring


/-- GROUND INSTANCE (official example shape [2,3,5,10]): specials [2,3] with sum-element 5 and
    outlier 10 — the array totals 20, and 20 − 2·5 recovers the outlier. -/
theorem vec : sol [2, 3] 10 = 20 := by decide

end LC.P3371
