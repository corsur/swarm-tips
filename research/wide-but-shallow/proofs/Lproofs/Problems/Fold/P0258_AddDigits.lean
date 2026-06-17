import Lproofs.Patterns
import Mathlib.Data.Nat.ModEq

/-! @lc 258 | name:Add Digits | scheme:fold | family:digit-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/add-digits/

    Repeatedly sum a number's digits until one digit remains (the digital root). CLASSIFICATION: the
    core step — summing the digits — is a streaming left fold (`+`-reduce, scalar accumulator).
    NON-VACUITY: we prove the digit sum is congruent to the number mod 9 (`digitSum ds ≡ toNat ds`),
    which is exactly why iterating it lands on the digital root. This pins the fold to genuine
    digit-aggregation work, not a re-encoding. We certify the fold + the congruence, not the O(1) form. -/

namespace LC.P0258
open Interview.Patterns

/-- Value of a big-endian digit list. -/
def toNat : List ℕ → ℕ
  | [] => 0
  | d :: ds => d * 10 ^ ds.length + toNat ds

/-- Sum of the digits — the fold the iterative algorithm repeats. -/
def digitSum (ds : List ℕ) : ℕ := ds.foldl (· + ·) 0

/-- SCHEME (fold): the digit sum is a left fold with a scalar accumulator. -/
theorem cls : IsFold (fun ds : List ℕ => ds.foldl (· + ·) 0) := ⟨(· + ·), 0, fun _ => rfl⟩

/-- `foldl (+)` accumulates linearly in its seed — the running-sum state is genuine incremental work. -/
theorem foldl_add (ds : List ℕ) (c : ℕ) : ds.foldl (· + ·) c = c + ds.foldl (· + ·) 0 := by
  induction ds generalizing c with
  | nil => simp
  | cons d ds ih => simp only [List.foldl_cons, Nat.zero_add]; rw [ih (c + d), ih d]; omega

theorem digitSum_cons (d : ℕ) (ds : List ℕ) : digitSum (d :: ds) = d + digitSum ds := by
  simp only [digitSum, List.foldl_cons, Nat.zero_add]
  exact foldl_add ds d

/-- NON-VACUITY (mod-9 congruence): the digit sum is congruent to the number modulo 9 — the reason
    iterating the digit-sum fold computes the digital root. Proven by induction, `10 ≡ 1 [MOD 9]`. -/
theorem corr (ds : List ℕ) : digitSum ds ≡ toNat ds [MOD 9] := by
  induction ds with
  | nil => rfl
  | cons d ds ih =>
    rw [digitSum_cons, toNat]
    have h10 : (10 : ℕ) ^ ds.length ≡ 1 [MOD 9] := by
      have : (10 : ℕ) ≡ 1 [MOD 9] := by decide
      simpa using this.pow ds.length
    have hd : d * 10 ^ ds.length ≡ d [MOD 9] := by
      simpa using Nat.ModEq.mul_left d h10
    exact (Nat.ModEq.add_left d ih).trans (Nat.ModEq.add_right (toNat ds) hd.symm)

end LC.P0258
