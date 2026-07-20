import Lproofs.Schemes.Fold

/-! @lc 2965 | name:Find Missing and Repeated Values | scheme:fold | family:hashing | complexity:O(n²) |
    source:https://leetcode.com/problems/find-missing-and-repeated-values/

    The grid holds `1..n²` except one value `a` is missing and another `b` appears twice. The accepted
    O(n²) solution recovers `a` and `b` from the running sum (and sum of squares). CLASSIFICATION
    (fold): the total is a streaming sum fold. CORRECTNESS: we certify the sum identity the solution
    inverts — replacing the missing value `a` by the repeated value `b` shifts the total by exactly
    `b − a`, so `sum(grid) = sum(1..n²) − a + b`. -/

namespace LC.P2965
open Interview.Patterns

/-- The grid total the single pass accumulates. -/
def sol (L : List ℤ) : ℤ := L.sum

/-- SCHEME (fold): the grid total `sol` is a streaming right-fold sum. -/
theorem cls : IsRightFold sol := by
  refine ⟨(· + ·), 0, fun L => ?_⟩
  unfold sol
  induction L with
  | nil => rfl
  | cons a t ih => simp [List.sum_cons, ih]

/-- CORRECT: starting from the complete list `base = 1..n²`, erasing the missing value `a` and adding
    the repeated value `b` shifts the total by exactly `b − a` — the identity the solution inverts. -/
theorem corr (base : List ℤ) (a b : ℤ) (ha : a ∈ base) :
    sol ((base.erase a) ++ [b]) = sol base - a + b := by
  simp only [sol]
  have hp : base.sum = a + (base.erase a).sum := by
    have h := (List.perm_cons_erase ha).sum_eq
    simpa [List.sum_cons] using h
  rw [List.sum_append, List.sum_cons, List.sum_nil]
  linarith


/-- GROUND INSTANCE (official example 1, grid [[1,3],[2,2]]): the grid total is 8, and the
    complete total 1..4 exceeds it by missing − repeated = 4 − 2. -/
theorem vec : sol [1, 3, 2, 2] = 8 ∧ sol [1, 2, 3, 4] - sol [1, 3, 2, 2] = 4 - 2 := by decide

end LC.P2965
