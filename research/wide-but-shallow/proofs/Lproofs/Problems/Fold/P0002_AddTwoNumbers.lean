import Lproofs.Problems.Fold.P0415_AddStrings

/-! @lc 2 | name:Add Two Numbers | scheme:fold | family:carry-propagation | complexity:O(n) |
    source:https://leetcode.com/problems/add-two-numbers/

    Two numbers are given as linked lists with the *least-significant digit first*, so the accepted
    pass is exactly the right-to-left base-10 carry propagation of Add Strings (415): at each node sum
    the two digits and the carry, emit `% 10`, carry `/ 10`. CLASSIFICATION: the same streaming
    carry-propagation fold. NON-VACUITY: the value identity `toNat (add a b c) = toNat a + toNat b + c`
    is inherited verbatim — the pass computes the genuine sum. -/

namespace LC.P0002

/-- The accepted addition is precisely Add Strings' little-endian carry pass. -/
def add : List ℕ → List ℕ → ℕ → List ℕ := LC.P0415.add

/-- SCHEME (fold / carry-propagation): the digit-by-digit carry recurrence. -/
theorem cls (x : ℕ) (xs : List ℕ) (y : ℕ) (ys : List ℕ) (c : ℕ) :
    add (x :: xs) (y :: ys) c = (x + y + c) % 10 :: add xs ys ((x + y + c) / 10) :=
  LC.P0415.cls x xs y ys c

/-- NON-VACUITY (value): the carry pass computes the sum of the two numbers and the incoming carry. -/
theorem corr (a b : List ℕ) (c : ℕ) :
    LC.P0415.toNat (add a b c) = LC.P0415.toNat a + LC.P0415.toNat b + c :=
  LC.P0415.corr a b c

end LC.P0002
