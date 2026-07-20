import Lproofs.Schemes.Fold

/-! @lc 2149 | name:Rearrange Array Elements by Sign | scheme:fold | family:two-pointers |
    complexity:O(n) | source:https://leetcode.com/problems/rearrange-array-elements-by-sign/

    With equally many positives and negatives, the array is rebuilt by alternating signs — interleaving
    the positives and negatives in their original relative order. CLASSIFICATION (fold): the rebuild is a
    streaming two-pointer sol. CORRECTNESS: we certify that the rearrangement loses nothing — the
    interleaved output has exactly as many entries as the two inputs combined, so every element is kept. -/

namespace LC.P2149

/-- Alternate-sign rebuild: sol positives and negatives. -/
def sol : List ℤ → List ℤ → List ℤ
  | a :: as, b :: bs => a :: b :: sol as bs
  | as, [] => as
  | [], bs => bs

/-- SCHEME (fold): each step emits one positive and one negative, then recurses (the streaming step). -/
theorem cls (x y : ℤ) (xs ys : List ℤ) :
    sol (x :: xs) (y :: ys) = x :: y :: sol xs ys := rfl

/-- CORRECT: the interleaved output keeps every element — its length is the sum of the two inputs'. -/
theorem corr (a b : List ℤ) : (sol a b).length = a.length + b.length := by
  induction a generalizing b with
  | nil => cases b <;> simp [sol]
  | cons x xs ih =>
    cases b with
    | nil => simp [sol]
    | cons y ys => simp only [sol, List.length_cons, ih]; omega


/-- GROUND INSTANCE (official example 1): positives [3,1,2] and negatives [-2,-5,-4] interleave
    to the judge's [3,-2,1,-5,2,-4]. -/
theorem vec : sol [3, 1, 2] [-2, -5, -4] = [3, -2, 1, -5, 2, -4] := by decide

end LC.P2149
