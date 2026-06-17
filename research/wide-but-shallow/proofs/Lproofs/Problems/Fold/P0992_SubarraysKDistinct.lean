import Lproofs.Schemes.Window

/-! @lc 992 | name:Subarrays with K Different Integers | scheme:fold | family:sliding-window |
    complexity:O(n) | source:https://leetcode.com/problems/subarrays-with-k-different-integers/

    Count = atMost(K) − atMost(K−1); each `atMost` is a sliding window that expands right and contracts
    left while the window holds more than the allowed number of distinct values. CLASSIFICATION: the
    window is a streaming left fold; the left pointer only advances, dropping a leading run (`p` marks a
    leading element the algorithm removes once the window exceeds capacity). NON-VACUITY: we certify the
    fold and that the window is always a contiguous subarray — so every window counted is a real
    subarray of the input. -/

namespace LC.P0992
open Interview.Schemes

variable (p : ℤ → Bool)

/-- Left-pointer contraction: drop the leading run removed when the window exceeds `K` distinct. -/
def shrink (w : List ℤ) : List ℤ := w.dropWhile p

/-- SCHEME (fold): the at-most-K sliding window is a streaming left fold. -/
theorem cls : Interview.Patterns.IsFold (windowFold (shrink p)) := windowFold_isFold _

/-- CORRECT (structural): the window is always a contiguous subarray of the input. -/
theorem corr (xs : List ℤ) : (windowFold (shrink p) xs) <:+ xs :=
  window_isSuffix _ (dropWhile_dropsFront _) xs

end LC.P0992
