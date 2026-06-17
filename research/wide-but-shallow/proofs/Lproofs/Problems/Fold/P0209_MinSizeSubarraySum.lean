import Lproofs.Patterns

/-! @lc 209 | name:Minimum Size Subarray Sum | scheme:fold | family:prefix-sum | complexity:O(n) |
    source:https://leetcode.com/problems/minimum-size-subarray-sum/

    Shortest subarray summing to at least a target; the sliding window rides the running sum.
    CLASSIFICATION: the window maintains a running sum, accumulated by a streaming left fold over the
    values. NON-VACUITY: we prove the running-sum recurrence — the sum over `xs ++ [x]` is the sum over
    `xs` plus `x` — so the accumulator carries genuine incremental window-sum state, not a re-encoding.
    We certify the fold + the prefix-sum recurrence; DROP the minimal-length value. -/

namespace LC.P0209
open Interview.Patterns

/-- Running window sum: fold the values with `+`. -/
def wsum (xs : List ℕ) : ℕ := xs.foldl (· + ·) 0

/-- SCHEME (fold): the running sum is a left fold with a scalar accumulator. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl (· + ·) 0) := ⟨(· + ·), 0, fun _ => rfl⟩

/-- The window sum accumulates linearly in its seed — genuine incremental state. -/
theorem foldl_add (xs : List ℕ) (c : ℕ) : xs.foldl (· + ·) c = c + xs.foldl (· + ·) 0 := by
  induction xs generalizing c with
  | nil => simp
  | cons x xs ih => simp only [List.foldl_cons, Nat.zero_add]; rw [ih (c + x), ih x]; omega

/-- NON-VACUITY (prefix-sum recurrence): extending the window by `x` adds `x` to its sum. -/
theorem corr (xs : List ℕ) (x : ℕ) : wsum (xs ++ [x]) = wsum xs + x := by
  simp only [wsum, List.foldl_append, List.foldl_cons, List.foldl_nil]

end LC.P0209
