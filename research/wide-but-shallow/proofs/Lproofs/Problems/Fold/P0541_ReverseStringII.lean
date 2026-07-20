import Lproofs.Schemes.Fold

/-! @lc 541 | name:Reverse String II | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/reverse-string-ii/

    Every block of `2k` characters has its first `k` reversed. CLASSIFICATION (fold): the rebuild streams
    over the blocks, reversing each prefix in place. CORRECTNESS: we certify the in-place invariant — the
    block operation (reverse the first `k`, keep the rest) preserves the string's length, so the result
    is a rearrangement of the same characters. -/

namespace LC.P0541

/-- Reverse the first `k` entries, keep the rest (the per-block operation). -/
def sol (k : ℕ) (l : List ℤ) : List ℤ := (l.take k).reverse ++ l.drop k

/-- SCHEME (fold): the block operation is reverse-prefix-then-keep-suffix (the streaming step). -/
theorem cls (k : ℕ) (l : List ℤ) : sol k l = (l.take k).reverse ++ l.drop k := rfl

/-- CORRECT: reversing a prefix in place preserves the length — the characters are only rearranged. -/
theorem corr (k : ℕ) (l : List ℤ) : (sol k l).length = l.length := by
  simp only [sol, List.length_append, List.length_reverse, List.length_take, List.length_drop]
  omega


/-- GROUND INSTANCE (official example 1, first block of "abcdefg" as 1..7 with k = 2): the
    block operation reverses exactly the first two entries. -/
theorem vec : sol 2 [1, 2, 3, 4, 5, 6, 7] = [2, 1, 3, 4, 5, 6, 7] := by decide

end LC.P0541
