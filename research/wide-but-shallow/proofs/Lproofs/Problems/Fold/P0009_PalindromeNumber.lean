import Lproofs.Patterns

/-! @lc 9 | name:Palindrome Number | scheme:fold | family:digit-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/palindrome-number/

    Reverse the digits and compare to the original. CLASSIFICATION: the digit-to-value step is a
    Horner left fold (`acc ↦ acc*10 + d`) with a scalar accumulator — exactly the running value the
    iterative algorithm maintains while consuming digits most-significant first. NON-VACUITY: we prove
    the fold equals the genuine positional value `toNat` of the digit list, so the accumulator carries
    real arithmetic state (defeats the carry-the-input vacuity). We certify the fold + the Horner
    correctness, not the palindrome decision. -/

namespace LC.P0009
open Interview.Patterns

/-- Value of a big-endian digit list (most significant first). -/
def toNat : List ℕ → ℕ
  | [] => 0
  | d :: ds => d * 10 ^ ds.length + toNat ds

/-- Horner step the iterative scan repeats: fold the digits into a running value. -/
def eval (ds : List ℕ) : ℕ := ds.foldl (fun a d => a * 10 + d) 0

/-- SCHEME (fold): the running value is a left fold with a scalar accumulator. -/
theorem cls : IsFold (fun ds : List ℕ => ds.foldl (fun a d => a * 10 + d) 0) :=
  ⟨(fun a d => a * 10 + d), 0, fun _ => rfl⟩

/-- The Horner fold accumulates positionally in its seed — genuine incremental arithmetic state. -/
theorem foldl_horner (ds : List ℕ) (c : ℕ) :
    ds.foldl (fun a d => a * 10 + d) c = c * 10 ^ ds.length + ds.foldl (fun a d => a * 10 + d) 0 := by
  induction ds generalizing c with
  | nil => simp
  | cons d ds ih =>
    simp only [List.foldl_cons, List.length_cons]
    rw [ih (c * 10 + d), ih (0 * 10 + d), pow_succ]
    ring

/-- NON-VACUITY: the Horner fold equals the positional value of the digit list. -/
theorem corr (ds : List ℕ) : eval ds = toNat ds := by
  induction ds with
  | nil => rfl
  | cons d ds ih =>
    rw [eval, List.foldl_cons, foldl_horner ds (0 * 10 + d), toNat]
    simp only [Nat.zero_mul, Nat.zero_add]
    rw [← eval, ih]

end LC.P0009
