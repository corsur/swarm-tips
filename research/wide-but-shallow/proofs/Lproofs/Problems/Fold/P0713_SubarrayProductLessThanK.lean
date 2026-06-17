import Lproofs.Patterns

/-! @lc 713 | name:Subarray Product Less Than K | scheme:fold | family:sliding-window | complexity:O(n) |
    source:https://leetcode.com/problems/subarray-product-less-than-k/

    Count subarrays whose product is below `k`; the sliding window rides a running product.
    CLASSIFICATION: the window maintains a running product, accumulated by a streaming left fold over the
    values. NON-VACUITY: we prove the running-product recurrence — the product over `xs ++ [x]` is the
    product over `xs` times `x` — so the accumulator carries genuine incremental window-product state,
    not a re-encoding. We certify the fold + the product recurrence; DROP the subarray count. -/

namespace LC.P0713
open Interview.Patterns

/-- Running window product: fold the values with `*`. -/
def wprod (xs : List ℕ) : ℕ := xs.foldl (· * ·) 1

/-- SCHEME (fold): the running product is a left fold with a scalar accumulator. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl (· * ·) 1) := ⟨(· * ·), 1, fun _ => rfl⟩

/-- The product accumulates multiplicatively in its seed — genuine incremental state. -/
theorem foldl_mul (xs : List ℕ) (c : ℕ) : xs.foldl (· * ·) c = c * xs.foldl (· * ·) 1 := by
  induction xs generalizing c with
  | nil => simp
  | cons x xs ih => simp only [List.foldl_cons, Nat.one_mul]; rw [ih (c * x), ih x]; ring

/-- NON-VACUITY (product recurrence): extending the window by `x` multiplies its product by `x`. -/
theorem corr (xs : List ℕ) (x : ℕ) : wprod (xs ++ [x]) = wprod xs * x := by
  simp only [wprod, List.foldl_append, List.foldl_cons, List.foldl_nil]

end LC.P0713
