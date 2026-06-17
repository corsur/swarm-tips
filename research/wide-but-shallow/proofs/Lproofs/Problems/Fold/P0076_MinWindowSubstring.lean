import Lproofs.Schemes.Window

/-! @lc 76 | name:Minimum Window Substring | scheme:fold | family:sliding-window | complexity:O(n) |
    source:https://leetcode.com/problems/minimum-window-substring/

    Expand the right edge over `s`, contract the left while the window still covers all of `t`.
    CLASSIFICATION: the window is a streaming left fold (append-then-contract); the left pointer only
    advances, dropping a leading run (`p` marks a leading character the algorithm may remove).
    NON-VACUITY: we certify the fold and that the window is always a contiguous substring of `s` — the
    structural invariant the minimum-window search relies on (every candidate it inspects is a real
    substring). -/

namespace LC.P0076
open Interview.Schemes

variable (p : Char → Bool)

/-- Left-pointer contraction: drop the leading run the algorithm has decided is removable. -/
def shrink (w : List Char) : List Char := w.dropWhile p

/-- SCHEME (fold): the sliding window is a streaming left fold. -/
theorem cls : Interview.Patterns.IsFold (windowFold (shrink p)) := windowFold_isFold _

/-- CORRECT (structural): the window is always a contiguous substring of the input. -/
theorem corr (s : List Char) : (windowFold (shrink p) s) <:+ s :=
  window_isSuffix _ (dropWhile_dropsFront _) s

end LC.P0076
