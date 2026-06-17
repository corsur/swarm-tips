import Lproofs.Patterns

/-! @lc 415 | name:Add Strings | scheme:fold | family:carry-propagation | complexity:O(n) |
    source:https://leetcode.com/problems/add-strings/

    Add two non-negative integers given as digit lists. CLASSIFICATION: the accepted right-to-left
    scan is a carry-propagation pass — at each position sum the two digits and the incoming carry,
    emit `% 10`, carry `/ 10`. NON-VACUITY: we prove the pass computes the genuine sum
    `toNat (add a b c) = toNat a + toNat b + c`, pinning the step to real base-10 addition. We certify
    the carry-propagation structure + the value, not any representation minimality. -/

namespace LC.P0415

/-- Value of a little-endian digit list. -/
def toNat : List ℕ → ℕ
  | [] => 0
  | d :: ds => d + 10 * toNat ds

/-- Little-endian digits of a number (used to flush a trailing carry of any size). -/
def digitsOf : ℕ → List ℕ
  | 0 => []
  | c + 1 => (c + 1) % 10 :: digitsOf ((c + 1) / 10)
  decreasing_by exact Nat.div_lt_self (Nat.succ_pos c) (by omega)

theorem toNat_digitsOf (c : ℕ) : toNat (digitsOf c) = c := by
  induction c using digitsOf.induct with
  | case1 => simp [digitsOf, toNat]
  | case2 k ih => rw [digitsOf, toNat, ih]; omega

/-- Right-to-left digit addition with a carry (little-endian). -/
def add : List ℕ → List ℕ → ℕ → List ℕ
  | [], [], c => digitsOf c
  | [], y :: ys, c => (y + c) % 10 :: add [] ys ((y + c) / 10)
  | x :: xs, [], c => (x + c) % 10 :: add xs [] ((x + c) / 10)
  | x :: xs, y :: ys, c => (x + y + c) % 10 :: add xs ys ((x + y + c) / 10)

/-- SCHEME (fold / carry-propagation): each position emits `(x+y+carry) % 10` and propagates
    `(x+y+carry) / 10` — the genuine streaming carry recurrence. -/
theorem cls (x : ℕ) (xs : List ℕ) (y : ℕ) (ys : List ℕ) (c : ℕ) :
    add (x :: xs) (y :: ys) c = (x + y + c) % 10 :: add xs ys ((x + y + c) / 10) := by
  simp [add]

/-- NON-VACUITY (value): the carry pass computes the sum of the two numbers and the incoming carry. -/
theorem corr : ∀ (a b : List ℕ) (c : ℕ), toNat (add a b c) = toNat a + toNat b + c := by
  intro a b c
  induction a, b, c using add.induct with
  | case1 c => simp [add, toNat, toNat_digitsOf]
  | case2 y ys c ih => simp only [add, toNat, ih]; omega
  | case3 x xs c ih => simp only [add, toNat, ih]; omega
  | case4 x xs y ys c ih => simp only [add, toNat, ih]; omega

end LC.P0415
