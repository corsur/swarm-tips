import Lproofs.Schemes.Fold

/-! @lc 1915 | name:Number of Wonderful Substrings | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/number-of-wonderful-substrings/

    A substring is "wonderful" if at most one character has an odd count. The O(n) solution carries a
    running parity bitmask (one bit per letter, toggled as the scan advances) and counts prefix masks.
    CLASSIFICATION (fold): the parity mask is a streaming XOR fold over the toggles. CORRECTNESS: we
    certify the prefix-XOR identity the whole method rests on — the parity mask of the substring `[i, j)`
    is exactly `sol j XOR sol i`, so two equal prefix masks bound an all-even substring. -/

namespace LC.P1915

/-- Running parity bitmask after `n` characters (`toggle k` flips the bit of the k-th character). -/
def sol (toggle : ℕ → ℕ) : ℕ → ℕ
  | 0 => 0
  | n + 1 => sol toggle n ^^^ toggle n

/-- XOR of the toggles over the window `[i, i+d)`. -/
def rangeXor (toggle : ℕ → ℕ) (i : ℕ) : ℕ → ℕ
  | 0 => 0
  | d + 1 => rangeXor toggle i d ^^^ toggle (i + d)

/-- SCHEME (fold): the parity mask is a streaming left fold of XOR-toggles over the positions. -/
theorem cls (toggle : ℕ → ℕ) (n : ℕ) :
    sol toggle n = (List.range n).foldl (fun m k => m ^^^ toggle k) 0 := by
  induction n with
  | zero => rfl
  | succ m ih => rw [sol, ih, List.range_succ, List.foldl_append]; rfl

/-- The prefix mask telescopes: extending by `d` toggles XORs in the window's combined parity. -/
theorem prefixMask_add (toggle : ℕ → ℕ) (i d : ℕ) :
    sol toggle (i + d) = sol toggle i ^^^ rangeXor toggle i d := by
  induction d with
  | zero => simp [rangeXor, Nat.xor_zero]
  | succ d ih =>
    have hij : i + (d + 1) = (i + d) + 1 := by omega
    rw [hij, sol, ih, rangeXor, Nat.xor_assoc]

/-- CORRECT: the parity mask of the substring `[i, j)` is `sol j XOR sol i` — so the
    substring is all-even iff the two prefix masks coincide, the identity the count exploits. -/
theorem corr (toggle : ℕ → ℕ) (i j : ℕ) (h : i ≤ j) :
    sol toggle j ^^^ sol toggle i = rangeXor toggle i (j - i) := by
  obtain ⟨d, rfl⟩ := Nat.exists_eq_add_of_le h
  rw [prefixMask_add, Nat.add_sub_cancel_left,
    Nat.xor_comm (sol toggle i) (rangeXor toggle i d), Nat.xor_assoc, Nat.xor_self,
    Nat.xor_zero]


/-- GROUND INSTANCE (official example 1, "aba" with a ↦ bit 0, b ↦ bit 1): the running parity
    masks are 0,1,3,2 — the full string's mask 2 has one odd letter (b), so "aba" is wonderful. -/
theorem vec : sol (fun k => [1, 2, 1].getD k 0) 3 = 2 ∧
    sol (fun k => [1, 2, 1].getD k 0) 2 = 3 := by decide

end LC.P1915
