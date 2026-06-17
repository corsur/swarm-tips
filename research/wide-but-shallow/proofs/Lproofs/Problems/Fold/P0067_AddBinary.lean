import Lproofs.Patterns

/-! @lc 67 | name:Add Binary | scheme:fold | family:carry-propagation | complexity:O(n) |
    source:https://leetcode.com/problems/add-binary/

    Add two binary strings. CLASSIFICATION: the accepted right-to-left scan is a base-2 carry
    propagation — at each position sum the two bits and the carry, emit `% 2`, carry `/ 2`.
    NON-VACUITY: we prove the pass computes the genuine sum `toNat (add a b c) = toNat a + toNat b + c`,
    pinning the step to real binary addition. We certify the carry-propagation + the value. -/

namespace LC.P0067

/-- Value of a little-endian binary digit list. -/
def toNat : List ℕ → ℕ
  | [] => 0
  | d :: ds => d + 2 * toNat ds

/-- Little-endian bits of a number (flushes a trailing carry of any size). -/
def bitsOf : ℕ → List ℕ
  | 0 => []
  | c + 1 => (c + 1) % 2 :: bitsOf ((c + 1) / 2)
  decreasing_by exact Nat.div_lt_self (Nat.succ_pos c) (by omega)

theorem toNat_bitsOf (c : ℕ) : toNat (bitsOf c) = c := by
  induction c using bitsOf.induct with
  | case1 => simp [bitsOf, toNat]
  | case2 k ih => rw [bitsOf, toNat, ih]; omega

/-- Right-to-left binary digit addition with a carry (little-endian). -/
def add : List ℕ → List ℕ → ℕ → List ℕ
  | [], [], c => bitsOf c
  | [], y :: ys, c => (y + c) % 2 :: add [] ys ((y + c) / 2)
  | x :: xs, [], c => (x + c) % 2 :: add xs [] ((x + c) / 2)
  | x :: xs, y :: ys, c => (x + y + c) % 2 :: add xs ys ((x + y + c) / 2)

/-- SCHEME (fold / carry-propagation): each position emits `(x+y+carry) % 2` and propagates
    `(x+y+carry) / 2` — the genuine streaming binary-carry recurrence. -/
theorem cls (x : ℕ) (xs : List ℕ) (y : ℕ) (ys : List ℕ) (c : ℕ) :
    add (x :: xs) (y :: ys) c = (x + y + c) % 2 :: add xs ys ((x + y + c) / 2) := by
  simp [add]

/-- NON-VACUITY (value): the carry pass computes the sum of the two numbers and the incoming carry. -/
theorem corr : ∀ (a b : List ℕ) (c : ℕ), toNat (add a b c) = toNat a + toNat b + c := by
  intro a b c
  induction a, b, c using add.induct with
  | case1 c => simp [add, toNat, toNat_bitsOf]
  | case2 y ys c ih => simp only [add, toNat, ih]; omega
  | case3 x xs c ih => simp only [add, toNat, ih]; omega
  | case4 x xs y ys c ih => simp only [add, toNat, ih]; omega

end LC.P0067
