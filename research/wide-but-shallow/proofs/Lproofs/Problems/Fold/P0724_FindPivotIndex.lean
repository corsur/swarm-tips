import Lproofs.Schemes.Fold

/-! @lc 724 | name:Find Pivot Index | scheme:fold | family:prefix-sum | complexity:O(n) |
    source:https://leetcode.com/problems/find-pivot-index/ -/

namespace LC.P0724
open Interview.Patterns

/-- A pivot index `i`: the prefix sum left of `i` equals the suffix sum right of `i`. -/
def isPivot (a : List ℤ) (i : ℕ) : Bool := decide ((a.take i).sum = (a.drop (i + 1)).sum)

/-- Editorial single pass over prefix sums: the leftmost pivot index, or `none`. -/
def sol (a : List ℤ) : Option ℕ := (List.range a.length).find? (isPivot a)

/-- Spec: `i` splits `a` into equal-sum halves. -/
def spec (a : List ℤ) (i : ℕ) : Prop := (a.take i).sum = (a.drop (i + 1)).sum

/-- SCHEME (scan): the running prefix sums driving the search are a streaming fold. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (· + ·) 0) := fold_prefixSum

/-- CORRECT: whenever the search returns an index, it is a genuine pivot. -/
theorem corr (a : List ℤ) {i : ℕ} (h : sol a = some i) : spec a i := by
  have hp : isPivot a i = true := List.find?_some h
  simpa [isPivot, spec] using hp

end LC.P0724
