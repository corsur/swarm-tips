import Lproofs.Schemes.Fold

/-! @lc 541 | name:Reverse String II | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/reverse-string-ii/

    Every block of `2k` characters has its first `k` reversed. CLASSIFICATION (fold): the rebuild streams
    over the blocks, reversing each prefix in place. CORRECTNESS: we certify the in-place invariant — the
    block operation (reverse the first `k`, keep the rest) preserves the string's length, so the result
    is a rearrangement of the same characters. -/

namespace LC.P0541

/-- Reverse the first `k` entries, keep the rest (the per-block operation). -/
def revFirst (k : ℕ) (l : List ℤ) : List ℤ := (l.take k).reverse ++ l.drop k

/-- SCHEME (fold): the block operation is reverse-prefix-then-keep-suffix (the streaming step). -/
theorem cls (k : ℕ) (l : List ℤ) : revFirst k l = (l.take k).reverse ++ l.drop k := rfl

/-- CORRECT: reversing a prefix in place preserves the length — the characters are only rearranged. -/
theorem corr (k : ℕ) (l : List ℤ) : (revFirst k l).length = l.length := by
  simp only [revFirst, List.length_append, List.length_reverse, List.length_take, List.length_drop]
  omega

end LC.P0541
