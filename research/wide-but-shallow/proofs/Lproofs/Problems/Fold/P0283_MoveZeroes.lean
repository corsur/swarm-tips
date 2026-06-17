import Lproofs.Schemes.Fold

/-! @lc 283 | name:Move Zeroes | scheme:fold | family:two-pointers | complexity:O(n) |
    source:https://leetcode.com/problems/move-zeroes/ -/

namespace LC.P0283
open Interview.Schemes

/-- The zeros of a list form a solid block of `0`s. -/
theorem filter_eq_zero_replicate (xs : List ℤ) :
    xs.filter (· == 0) = List.replicate (xs.countP (· == 0)) 0 := by
  induction xs with
  | nil => simp
  | cons x xs ih =>
    rcases eq_or_ne x 0 with hx | hx
    · subst hx; simp [List.filter_cons, List.countP_cons, ih, List.replicate_succ]
    · have : (x == 0) = false := by simpa using hx
      simp [List.filter_cons, List.countP_cons, this, ih]

/-- The non-zero compaction (the editorial two-pointer write pass). -/
def compact (a : List ℤ) : List ℤ := a.foldl (fun acc x => if x != 0 then acc ++ [x] else acc) []

/-- Editorial: compact the non-zeros to the front, fill the rest with zeros. -/
def sol (a : List ℤ) : List ℤ := compact a ++ List.replicate (a.countP (· == 0)) 0

/-- Spec: the non-zeros (order preserved) followed by all the zeros. -/
def spec (a : List ℤ) : List ℤ := a.filter (· != 0) ++ a.filter (· == 0)

/-- SCHEME (fold): the non-zero compaction is a streaming fold. -/
theorem cls : IsFold compact := ⟨_, [], fun _ => rfl⟩

/-- CORRECT: the compaction-plus-fill equals non-zeros-then-zeros. -/
theorem corr (a : List ℤ) : sol a = spec a := by
  rw [sol, spec, compact, foldl_filter_append, List.nil_append, ← filter_eq_zero_replicate]

end LC.P0283
