import Lproofs.Patterns
import Mathlib.Tactic

/-! @lc 50 | name:Pow(x, n) | scheme:fold | family:fast-exponentiation | complexity:O(log n) |
    source:https://leetcode.com/problems/powx-n/

    Compute `x^n`. CLASSIFICATION: the accepted fast-exponentiation scan is a streaming left fold over
    the bits of `n` (LSB first), carrying a bounded `(result, x^(2^i))` accumulator: each step squares
    the running power and multiplies it into the result when the bit is set. NON-VACUITY: we prove the
    fold computes `x ^ (value of the bits)`, pinning the squaring/multiply steps to genuine
    exponentiation work. We certify the fold + the power identity (the |n|<0 reciprocal is a wrapper). -/

namespace LC.P0050
open Interview.Patterns

/-- One fold step: square the running power; fold it into the result iff this bit is set. -/
def step (st : ℤ × ℤ) (b : Bool) : ℤ × ℤ := (if b then st.1 * st.2 else st.1, st.2 * st.2)

/-- The accepted fast-power scan over the exponent bits (LSB first), reading off the result. -/
def fastPow (x : ℤ) (bits : List Bool) : ℤ := (bits.foldl step (1, x)).1

/-- Value of a bit list, least-significant first. -/
def bitsVal : List Bool → ℕ
  | [] => 0
  | b :: bs => (if b then 1 else 0) + 2 * bitsVal bs

/-- SCHEME (fold): the scan is a streaming left fold over the bits with a bounded `(result, pow)` state. -/
theorem cls (x : ℤ) : IsFold (fun bits : List Bool => bits.foldl step (1, x)) :=
  ⟨step, (1, x), fun _ => rfl⟩

/-- The running accumulator after folding `bits` from `(r, p)` is `r * p ^ (bits value)`. -/
theorem foldl_step (bits : List Bool) (r p : ℤ) :
    (bits.foldl step (r, p)).1 = r * p ^ bitsVal bits := by
  induction bits generalizing r p with
  | nil => simp [bitsVal]
  | cons b bs ih =>
    rw [List.foldl_cons, ih, bitsVal]
    cases b
    · simp only [step, if_false, Bool.false_eq_true, pow_add, pow_mul]
      ring
    · simp only [step, if_true, pow_add, pow_mul]
      ring

/-- NON-VACUITY (power identity): the fast-power fold computes `x` raised to the bit-encoded exponent. -/
theorem corr (x : ℤ) (bits : List Bool) : fastPow x bits = x ^ bitsVal bits := by
  rw [fastPow, foldl_step, one_mul]

end LC.P0050
