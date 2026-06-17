import Lproofs.Patterns

/-! @lc 231 | name:Power of Two | scheme:fold | family:bit-aggregate | complexity:O(log n) |
    source:https://leetcode.com/problems/power-of-two/

    A positive integer is a power of two iff its binary representation has exactly one set bit.
    CLASSIFICATION: counting the set bits is a streaming left fold over the bit list with a scalar
    accumulator (the running popcount). NON-VACUITY: we prove the popcount never exceeds the number of
    bits — each bit contributes at most one — so the accumulator does genuine bit-counting work; the
    power-of-two test is `popcount = 1`. We certify the fold + the count bound. -/

namespace LC.P0231
open Interview.Patterns

/-- Count the set bits — the fold the bit-trick repeats. -/
def popcount (bits : List Bool) : ℕ := bits.foldl (fun acc b => if b then acc + 1 else acc) 0

/-- SCHEME (fold): the popcount is a left fold with a scalar accumulator. -/
theorem cls : IsFold (fun bits : List Bool => bits.foldl (fun acc b => if b then acc + 1 else acc) 0) :=
  ⟨(fun acc b => if b then acc + 1 else acc), 0, fun _ => rfl⟩

/-- The popcount accumulates by at most one per bit. -/
theorem foldl_bound (bits : List Bool) (acc : ℕ) :
    bits.foldl (fun a b => if b then a + 1 else a) acc ≤ acc + bits.length := by
  induction bits generalizing acc with
  | nil => simp
  | cons b bs ih =>
    simp only [List.foldl_cons, List.length_cons]
    have h := ih (if b then acc + 1 else acc)
    have hb : (if b then acc + 1 else acc) ≤ acc + 1 := by split <;> omega
    omega

/-- NON-VACUITY: the popcount never exceeds the number of bits (each bit counts at most once);
    a power of two is exactly the case `popcount = 1`. -/
theorem corr (bits : List Bool) : popcount bits ≤ bits.length := by
  have := foldl_bound bits 0
  simpa [popcount] using this

end LC.P0231
