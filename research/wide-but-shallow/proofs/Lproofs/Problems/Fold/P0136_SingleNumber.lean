import Lproofs.Schemes.Fold

/-! @lc 136 | name:Single Number | scheme:fold | family:math-bit | complexity:O(n) |
    source:https://leetcode.com/problems/single-number/

    Every value appears twice except one. XOR-reduce the array: equal values cancel (`x ^^^ x = 0`),
    leaving the lone element. CLASSIFICATION: the accepted solution IS a streaming left fold over the
    XOR monoid — `sol = foldl (· ^^^ ·) 0`. We close the scheme/correctness gap fully here: `cls` is the
    fold itself and `corr` proves the fold returns the unique element, so the certificate is the strong
    form ("the accepted code is this fold, and it is correct"), not two separate facts. -/

namespace LC.P0136
open Interview.Patterns

/-- Accepted O(n) solution: XOR-reduce — a streaming left fold over `(ℕ, ^^^, 0)`. -/
def sol (a : List ℕ) : ℕ := a.foldl (· ^^^ ·) 0

/-- SCHEME (fold): the solution is literally a left fold — no separate witness needed. -/
theorem cls : IsFold sol := ⟨(· ^^^ ·), 0, fun _ => rfl⟩

/-- XOR is right-commutative, so the left fold is permutation-invariant. -/
instance xor_rcomm : Std.Commutative (· ^^^ · : ℕ → ℕ → ℕ) := ⟨Nat.xor_comm⟩
instance xor_assoc : Std.Associative (· ^^^ · : ℕ → ℕ → ℕ) := ⟨Nat.xor_assoc⟩

/-- A doubled list XOR-folds to the seed: each equal pair cancels. -/
theorem pairs_fold (ys : List ℕ) : ∀ s, (ys.flatMap fun p => [p, p]).foldl (· ^^^ ·) s = s := by
  induction ys with
  | nil => intro s; simp
  | cons p ps ih =>
    intro s
    rw [List.flatMap_cons, List.foldl_append]
    have hpp : [p, p].foldl (· ^^^ ·) s = s := by
      simp only [List.foldl_cons, List.foldl_nil]
      rw [Nat.xor_assoc, Nat.xor_self, Nat.xor_zero]
    rw [hpp, ih]

/-- CORRECT: when every value appears twice except `t` (which appears once), the XOR fold returns `t`. -/
theorem corr {a : List ℕ} {t : ℕ} {ys : List ℕ}
    (h : a.Perm ((ys.flatMap fun p => [p, p]) ++ [t])) : sol a = t := by
  unfold sol
  rw [h.foldl_eq, List.foldl_append, pairs_fold]
  simp

end LC.P0136
