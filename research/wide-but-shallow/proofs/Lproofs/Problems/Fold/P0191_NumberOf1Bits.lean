import Lproofs.Schemes.Fold

/-! @lc 191 | name:Number of 1 Bits | scheme:fold | family:math-bit | complexity:O(1) |
    source:https://leetcode.com/problems/number-of-1-bits/

    The Hamming weight (popcount) is the number of set bits. CLASSIFICATION: the natural answer
    `(bits.filter id).length` is exhibited as a streaming left fold whose accumulator is a single
    running `ℕ` counter (genuine O(1) bounded state) — a non-vacuous fold membership (the count `sol`
    is defined independently and PROVEN equal to a `foldl`, not written as one). -/

namespace LC.P0191
open Interview.Patterns

/-- The natural popcount: how many bits are set. -/
def sol (bits : List Bool) : ℕ := (bits.filter id).length

/-- The running-counter fold computes the popcount (the non-vacuous recurrence). -/
theorem foldl_count (bits : List Bool) (c : ℕ) :
    bits.foldl (fun acc b => if b then acc + 1 else acc) c = c + (bits.filter id).length := by
  induction bits generalizing c with
  | nil => simp
  | cons b bs ih => rw [List.foldl_cons, ih]; cases b <;> simp [List.filter_cons] <;> omega

/-- SCHEME (fold): popcount is a streaming left fold with a bounded `ℕ` accumulator. -/
theorem cls : IsFold (fun bits : List Bool => (bits.filter id).length) :=
  ⟨fun acc b => if b then acc + 1 else acc, 0, fun bits => by rw [foldl_count bits 0, Nat.zero_add]⟩

theorem corr (bits : List Bool) : sol bits = (bits.filter id).length := rfl

end LC.P0191
