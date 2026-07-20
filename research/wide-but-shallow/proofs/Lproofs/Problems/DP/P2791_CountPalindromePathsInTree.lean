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
def sol : List ℕ → ℕ
  | [] => 0
  | e :: es => e ^^^ sol es

/-- XOR of the bits on a contiguous segment of `j` edges starting at offset into `es`. -/
def segMask (es : List ℕ) (j : ℕ) : ℕ := sol (es.take j)

/-- SCHEME (dp / tree fold): the root mask folds the edge bits with XOR --- one bit per edge. -/
theorem cls (e : ℕ) (es : List ℕ) : sol (e :: es) = e ^^^ sol es := rfl

theorem rootMask_append (a b : List ℕ) : sol (a ++ b) = sol a ^^^ sol b := by
  induction a with
  | nil => simp [sol]
  | cons e es ih => simp only [List.cons_append, sol, ih]; rw [Nat.xor_assoc]

/-- CORRECT: the path mask telescopes --- the root mask of a descendant is the root mask of an
    ancestor XOR the segment between them. This is the prefix-XOR identity (`segment = mask u XOR
    mask v`) the palindrome test rests on. -/
theorem corr (es : List ℕ) (j : ℕ) (hj : j ≤ es.length) :
    sol es = sol (es.take j) ^^^ sol (es.drop j) := by
  conv_lhs => rw [← List.take_append_drop j es]
  rw [rootMask_append]


/-- GROUND INSTANCE: the edge-bit path [1,2,1] has parity mask 2 (the 1-bits cancel — one letter
    odd, so the path is palindrome-permutable); [1,1] cancels fully to 0. -/
theorem vec : sol [1, 2, 1] = 2 ∧ sol [1, 1] = 0 := by decide

end LC.P2791
