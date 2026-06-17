import Lproofs.Schemes.Fold

/-! @lc 560 | name:Subarray Sum Equals K | scheme:fold | family:prefix-sum | complexity:O(n) |
    source:https://leetcode.com/problems/subarray-sum-equals-k/

    Count subarrays summing to `k`. The accepted O(n) solution streams a running prefix sum and a
    hash map of how often each prefix value has occurred. CLASSIFICATION: the prefix-sum sequence is a
    streaming left scan (`scanl (+) 0`) — a left fold recording the running total. NON-VACUITY: we
    prove the telescoping foundation the whole method rests on — the `i`-th prefix equals the sum of
    the first `i` elements — so a window's sum is a difference of two prefixes. -/

namespace LC.P0560

/-- Running prefix sums `[0, x₀, x₀+x₁, …]` — the streaming left scan. -/
def prefixSums (xs : List ℤ) : List ℤ := xs.scanl (· + ·) 0

/-- SCHEME (fold): the prefix sums are a streaming left scan — the head is the empty-prefix `0`, then
    the scan of the tail seeded with the first element. -/
theorem cls (x : ℤ) (xs : List ℤ) : prefixSums (x :: xs) = 0 :: xs.scanl (· + ·) x := by
  simp [prefixSums, List.scanl_cons]

/-- A left scan seeded with `s`, read at index `i`, is `s` plus the sum of the first `i` elements. -/
theorem scanl_get (xs : List ℤ) (s : ℤ) (i : ℕ) (hi : i ≤ xs.length) :
    (xs.scanl (· + ·) s)[i]? = some (s + (xs.take i).sum) := by
  induction xs generalizing s i with
  | nil =>
    simp only [List.length_nil, Nat.le_zero] at hi
    subst hi
    simp
  | cons x xs ih =>
    cases i with
    | zero => simp [List.scanl_cons]
    | succ j =>
      rw [List.scanl_cons]
      simp only [List.getElem?_cons_succ, List.take_succ_cons, List.sum_cons]
      rw [ih (s + x) j (by simp only [List.length_cons] at hi; omega)]
      simp only [Option.some.injEq]
      ring

/-- NON-VACUITY (telescoping): the `i`-th prefix sum equals the sum of the first `i` elements, so any
    window's sum is a difference of two prefixes — the basis of the prefix-sum count. -/
theorem corr (xs : List ℤ) (i : ℕ) (hi : i ≤ xs.length) :
    (prefixSums xs)[i]? = some (xs.take i).sum := by
  rw [prefixSums, scanl_get xs 0 i hi, zero_add]

end LC.P0560
