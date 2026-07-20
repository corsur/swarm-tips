import Lproofs.Patterns

/-! @lc 7 | name:Reverse Integer | scheme:fold | family:digit-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/reverse-integer/

    Reverse the digits of an integer by repeatedly popping the last digit (`% 10`) and Horner-folding it
    into a running value (`acc * 10 + d`). CLASSIFICATION: that running value is a streaming left fold
    with a scalar accumulator. NON-VACUITY: we prove the Horner fold equals the genuine positional value
    of the processed digit sequence, so the accumulator carries real arithmetic state (defeats the
    carry-the-input vacuity). We certify the fold + the Horner correctness; DROP the overflow clamp. -/

namespace LC.P0007
open Interview.Patterns

/-- Positional value of a big-endian digit list. -/
def toNat : List ℕ → ℕ
  | [] => 0
  | d :: ds => d * 10 ^ ds.length + toNat ds

/-- Horner step the reversal repeats: fold popped digits into a running value. -/
def sol (ds : List ℕ) : ℕ := ds.foldl (fun a d => a * 10 + d) 0

/-- SCHEME (fold): the running value is a left fold with a scalar accumulator — and `sol` is
    exactly that fold. -/
theorem cls : IsFold (fun ds : List ℕ => ds.foldl (fun a d => a * 10 + d) 0) ∧
    ∀ ds : List ℕ, sol ds = ds.foldl (fun a d => a * 10 + d) 0 :=
  ⟨⟨(fun a d => a * 10 + d), 0, fun _ => rfl⟩, fun _ => rfl⟩

/-- The Horner fold accumulates positionally in its seed — genuine incremental arithmetic state. -/
theorem foldl_horner (ds : List ℕ) (c : ℕ) :
    ds.foldl (fun a d => a * 10 + d) c = c * 10 ^ ds.length + ds.foldl (fun a d => a * 10 + d) 0 := by
  induction ds generalizing c with
  | nil => simp
  | cons d ds ih =>
    simp only [List.foldl_cons, List.length_cons]
    rw [ih (c * 10 + d), ih (0 * 10 + d), pow_succ]
    ring

/-- NON-VACUITY: the Horner fold equals the positional value of the processed digit sequence. -/
theorem corr (ds : List ℕ) : sol ds = toNat ds := by
  induction ds with
  | nil => rfl
  | cons d ds ih =>
    rw [sol, List.foldl_cons, foldl_horner ds (0 * 10 + d), toNat]
    simp only [Nat.zero_mul, Nat.zero_add]
    rw [← sol, ih]


/-- GROUND INSTANCE (official example 1): reversing 123 pops digits 3,2,1 into 321. -/
theorem vec : sol [3, 2, 1] = 321 := by decide

end LC.P0007
