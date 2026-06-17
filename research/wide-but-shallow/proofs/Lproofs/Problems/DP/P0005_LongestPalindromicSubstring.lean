import Lproofs.Schemes.Fold

/-! @lc 5 | name:Longest Palindromic Substring | scheme:dp | family:interval-dp | complexity:O(n²) |
    source:https://leetcode.com/problems/longest-palindromic-substring/

    Interval DP marking which substrings `s i … s j` are palindromes. CLASSIFICATION: a recursive
    decomposition on the interval — `s i … s j` is a palindrome iff its ends match and its interior
    `(i+1, j-1)` is a palindrome (`cls`). NON-VACUITY: we prove the genuine base case — every
    single-character interval is a palindrome (`corr`) — pinning the DP to real interval work. We certify
    the interval recurrence + the base; DROP the longest-substring extraction. -/

namespace LC.P0005

/-- Whether the interval `[i, j]` of `s` is a palindrome (fuel bounds recursion). -/
def pal (s : ℕ → Char) : ℕ → ℕ → ℕ → Bool
  | 0, _, _ => true
  | f + 1, i, j => if j ≤ i then true else (s i == s j && pal s f (i + 1) (j - 1))

/-- SCHEME (dp / interval recurrence): a substring is a palindrome iff its ends match and its interior
    `(i+1, j-1)` is a palindrome. -/
theorem cls (s : ℕ → Char) (f i j : ℕ) (hij : i < j) :
    pal s (f + 1) i j = (s i == s j && pal s f (i + 1) (j - 1)) := by
  simp only [pal]
  rw [if_neg (by omega : ¬ (j ≤ i))]

/-- NON-VACUITY (base): every single-character interval is a palindrome. -/
theorem corr (s : ℕ → Char) (f i : ℕ) : pal s (f + 1) i i = true := by simp [pal]

end LC.P0005
