import Lproofs.Schemes.Fold

/-! @lc 303 | name:Range Sum Query - Immutable | scheme:fold | family:prefix-sum | complexity:O(1) query |
    source:https://leetcode.com/problems/range-sum-query-immutable/ -/

namespace LC.P0303
open Interview.Patterns

/-- Prefix total of the first `k` elements. -/
def pre (a : List ℤ) (k : ℕ) : ℤ := (a.take k).sum

/-- Editorial solution: precompute prefix sums, answer each query by a difference (O(1)). -/
def sol (a : List ℤ) (i j : ℕ) : ℤ := pre a j - pre a i

/-- Spec: the sum of the subarray `a[i..j)`. -/
def spec (a : List ℤ) (i j : ℕ) : ℤ := ((a.drop i).take (j - i)).sum

/-- SCHEME (scan): the prefix totals are a streaming fold (running accumulator). -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (· + ·) 0) := fold_prefixSum

/-- CORRECT: a prefix difference equals the subarray sum (for `i ≤ j`). -/
theorem corr (a : List ℤ) {i j : ℕ} (h : i ≤ j) : sol a i j = spec a i j := by
  obtain ⟨k, rfl⟩ : ∃ k, j = i + k := ⟨j - i, by omega⟩
  simp only [sol, spec, pre, Nat.add_sub_cancel_left, List.take_add, List.sum_append]
  ring

end LC.P0303
