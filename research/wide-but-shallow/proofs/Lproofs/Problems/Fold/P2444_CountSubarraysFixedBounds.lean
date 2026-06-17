import Lproofs.Schemes.Window

/-! @lc 2444 | name:Count Subarrays With Fixed Bounds | scheme:fold | family:sliding-window |
    complexity:O(n) | source:https://leetcode.com/problems/count-subarrays-with-fixed-bounds/

    Scan left to right tracking the last out-of-range index and the last positions of `minK`/`maxK`;
    the count of valid subarrays ending at each index is a sliding window whose left edge advances past
    any out-of-range element. CLASSIFICATION: the window is a streaming left fold; the left pointer only
    advances, dropping a leading run (`p` marks a leading element jumped past on an out-of-range hit).
    NON-VACUITY: we certify the fold and that the window is always a contiguous subarray of the input. -/

namespace LC.P2444
open Interview.Schemes

variable (p : ℤ → Bool)

/-- Left-pointer contraction: drop the leading run skipped past on an out-of-range element. -/
def shrink (w : List ℤ) : List ℤ := w.dropWhile p

/-- SCHEME (fold): the bounded-window scan is a streaming left fold. -/
theorem cls : Interview.Patterns.IsFold (windowFold (shrink p)) := windowFold_isFold _

/-- CORRECT (structural): the window is always a contiguous subarray of the input. -/
theorem corr (xs : List ℤ) : (windowFold (shrink p) xs) <:+ xs :=
  window_isSuffix _ (dropWhile_dropsFront _) xs

end LC.P2444
