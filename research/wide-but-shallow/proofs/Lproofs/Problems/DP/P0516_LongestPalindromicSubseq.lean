import Lproofs.Schemes.Fold

/-! @lc 516 | name:Longest Palindromic Subsequence | scheme:dp | family:interval-dp | complexity:O(n²) |
    source:https://leetcode.com/problems/longest-palindromic-subsequence/

    Interval DP over the string indices `s i … s j`. CLASSIFICATION: a recursive decomposition on the
    interval — if the ends match, take both and recurse on the shrunk interval `(i+1, j-1)`; otherwise
    recurse on the two sub-intervals dropping one end and take the better (`cls`). NON-VACUITY: we prove
    the genuine base case — a single-character interval has palindromic subsequence length `1` (`corr`)
    — pinning the DP to real interval work. We certify the interval recurrence + the base; DROP the
    optimal length value. -/

namespace LC.P0516

/-- Longest palindromic subsequence length over the interval `[i, j]` of `s` (fuel bounds recursion). -/
def sol (s : ℕ → Char) : ℕ → ℕ → ℕ → ℕ
  | 0, _, _ => 0
  | f + 1, i, j =>
      if j ≤ i then (if i = j then 1 else 0)
      else if s i = s j then 2 + sol s f (i + 1) (j - 1)
      else max (sol s f (i + 1) j) (sol s f i (j - 1))

/-- SCHEME (dp / interval recurrence): when the ends match, take both and recurse on `(i+1, j-1)`. -/
theorem cls (s : ℕ → Char) (f i j : ℕ) (hij : i < j) (heq : s i = s j) :
    sol s (f + 1) i j = 2 + sol s f (i + 1) (j - 1) := by
  simp only [sol]
  rw [if_neg (by omega : ¬ (j ≤ i)), if_pos heq]

/-- NON-VACUITY (base): a single-character interval has palindromic-subsequence length one. -/
theorem corr (s : ℕ → Char) (f i : ℕ) : sol s (f + 1) i i = 1 := by simp [sol]


/-- GROUND INSTANCE (official example 1): the longest palindromic subsequence of "bbbab" has
    length 4 ("bbbb"). -/
theorem vec : sol (fun i => "bbbab".toList.getD i 'x') 5 0 4 = 4 := by decide

end LC.P0516
