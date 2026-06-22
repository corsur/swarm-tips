import Lproofs.Schemes.Fold

/-! @lc 2791 | name:Count Paths That Can Form a Palindrome in a Tree | scheme:dp | family:dp-tree |
    complexity:O(n) | source:https://leetcode.com/problems/count-paths-that-can-form-a-palindrome-in-a-tree/

    Each edge carries a character; a root-to-node parity bitmask (one bit per character, XORed down the
    tree) lets the accepted solution test a path for palindrome-ability via `mask(u) XOR mask(v)`.
    CLASSIFICATION (dp): the root mask is a tree fold of XOR-bits. CORRECTNESS (the prefix-XOR
    telescoping the algorithm relies on, not the final palindrome count): the mask accumulated along a
    root path telescopes --- extending the path by `j` edges XORs in exactly those edges' bits, so the
    segment mask between two collinear nodes is the XOR of their root masks. -/

namespace LC.P2791

/-- Root-to-node parity mask after walking the edge-bit list `es` (one XOR-bit per edge). -/
def rootMask : List ℕ → ℕ
  | [] => 0
  | e :: es => e ^^^ rootMask es

/-- XOR of the bits on a contiguous segment of `j` edges starting at offset into `es`. -/
def segMask (es : List ℕ) (j : ℕ) : ℕ := rootMask (es.take j)

/-- SCHEME (dp / tree fold): the root mask folds the edge bits with XOR --- one bit per edge. -/
theorem cls (e : ℕ) (es : List ℕ) : rootMask (e :: es) = e ^^^ rootMask es := rfl

theorem rootMask_append (a b : List ℕ) : rootMask (a ++ b) = rootMask a ^^^ rootMask b := by
  induction a with
  | nil => simp [rootMask]
  | cons e es ih => simp only [List.cons_append, rootMask, ih]; rw [Nat.xor_assoc]

/-- CORRECT: the path mask telescopes --- the root mask of a descendant is the root mask of an
    ancestor XOR the segment between them. This is the prefix-XOR identity (`segment = mask u XOR
    mask v`) the palindrome test rests on. -/
theorem corr (es : List ℕ) (j : ℕ) (hj : j ≤ es.length) :
    rootMask es = rootMask (es.take j) ^^^ rootMask (es.drop j) := by
  conv_lhs => rw [← List.take_append_drop j es]
  rw [rootMask_append]

end LC.P2791
