import Lproofs.Patterns

/-! @lc 1171 | name:Remove Zero Sum Consecutive Nodes from Linked List | scheme:fold |
    family:prefix-sum | complexity:O(n) |
    source:https://leetcode.com/problems/remove-zero-sum-consecutive-nodes-from-linked-list/

    Repeatedly delete consecutive runs summing to zero; the accepted O(n) solution walks the prefix sums
    and drops the segment between two equal prefixes. CLASSIFICATION: the prefix-sum walk is a streaming
    left fold over the values, accumulating the running prefix sum (a scalar). NON-VACUITY: we prove the
    running-sum recurrence — the prefix sum over `xs ++ [x]` is the prefix sum over `xs` plus `x` — so
    the accumulator carries genuine incremental prefix-sum state (equal prefixes ⇔ a zero-sum run). We
    certify the fold + the prefix-sum recurrence. -/

namespace LC.P1171
open Interview.Patterns

/-- Running prefix sum: fold the values with `+`. -/
def psum (xs : List ℤ) : ℤ := xs.foldl (· + ·) 0

/-- SCHEME (fold): the prefix-sum walk is a left fold with a scalar accumulator. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (· + ·) 0) := ⟨(· + ·), 0, fun _ => rfl⟩

/-- The prefix sum accumulates linearly in its seed — genuine incremental state. -/
theorem foldl_add (xs : List ℤ) (c : ℤ) : xs.foldl (· + ·) c = c + xs.foldl (· + ·) 0 := by
  induction xs generalizing c with
  | nil => simp
  | cons x xs ih => simp only [List.foldl_cons, zero_add]; rw [ih (c + x), ih x]; ring

/-- NON-VACUITY (prefix-sum recurrence): extending by `x` adds `x` to the prefix sum — equal prefix
    sums bracket a zero-sum run, which is exactly what the algorithm deletes. -/
theorem corr (xs : List ℤ) (x : ℤ) : psum (xs ++ [x]) = psum xs + x := by
  simp only [psum, List.foldl_append, List.foldl_cons, List.foldl_nil]

end LC.P1171
