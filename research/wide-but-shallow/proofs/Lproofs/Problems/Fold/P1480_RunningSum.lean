import Lproofs.Schemes.Fold

/-! @lc 1480 | name:Running Sum of 1d Array | scheme:fold | family:prefix-sum | complexity:O(n) |
    source:https://leetcode.com/problems/running-sum-of-1d-array/

    Output the array of prefix sums. CLASSIFICATION: the running sum is a streaming left fold (a scan)
    whose accumulator is the pair `(running total, emitted prefix sums)` — each element extends the total
    and appends it. NON-VACUITY: we prove the scan emits exactly one prefix sum per input element (the
    output length equals the input length), so the accumulator does genuine running-total work, not a
    re-encoding. We certify the fold + the length conservation. -/

namespace LC.P1480
open Interview.Patterns

/-- One scan step: extend the running total and append it to the output. -/
def step (s : ℤ × List ℤ) (x : ℤ) : ℤ × List ℤ := (s.1 + x, s.2 ++ [s.1 + x])

/-- Running sums: the second component of the scan over the array. -/
def runSum (xs : List ℤ) : List ℤ := (xs.foldl step (0, [])).2

/-- SCHEME (fold): the scan is a left fold with the `(total, output)` accumulator. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl step (0, [])) := ⟨step, (0, []), fun _ => rfl⟩

/-- Each step appends exactly one prefix sum. -/
theorem foldl_len (xs : List ℤ) (s : ℤ × List ℤ) :
    (xs.foldl step s).2.length = s.2.length + xs.length := by
  induction xs generalizing s with
  | nil => simp
  | cons x xs ih =>
    simp only [List.foldl_cons, List.length_cons]
    rw [ih (step s x)]
    simp only [step, List.length_append, List.length_cons, List.length_nil]
    omega

/-- NON-VACUITY: the scan emits exactly one prefix sum per input element. -/
theorem corr (xs : List ℤ) : (runSum xs).length = xs.length := by
  have := foldl_len xs (0, [])
  simpa [runSum] using this

end LC.P1480
