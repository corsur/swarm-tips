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
def total (specials : List ℤ) (outlier : ℤ) : ℤ := (specials ++ [specials.sum, outlier]).sum

/-- SCHEME (fold): the array total is a streaming right-fold sum. -/
theorem cls : IsRightFold (List.sum : List ℤ → ℤ) := by
  refine ⟨(· + ·), 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons a t ih => simp [List.sum_cons, ih]

/-- CORRECT: the outlier equals the total minus twice the special sum — the identity that lets a single
    pass recover the outlier from any candidate sum-element. -/
theorem corr (specials : List ℤ) (outlier : ℤ) :
    outlier = total specials outlier - 2 * specials.sum := by
  simp only [total, List.sum_append, List.sum_cons, List.sum_nil]
  ring

end LC.P3371
