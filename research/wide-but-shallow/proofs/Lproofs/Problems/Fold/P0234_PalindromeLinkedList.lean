import Lproofs.Patterns

/-! @lc 234 | name:Palindrome Linked List | scheme:fold | family:list-reverse | complexity:O(n) |
    source:https://leetcode.com/problems/palindrome-linked-list/

    A list is a palindrome iff it equals its reversal; the accepted O(1)-space solution reverses the
    second half and compares. CLASSIFICATION: the reversal at the core is a streaming left fold whose
    accumulator is the reversed-so-far prefix. NON-VACUITY: we prove the fold equals the genuine
    `List.reverse`, so the palindrome test `s = rev s` compares against a real reversal. We certify the
    fold + the reversal correctness; the palindrome decision is `s = rev s`. -/

namespace LC.P0234
open Interview.Patterns

/-- Reverse by cons-folding onto the accumulator (the in-place half-reversal's core). -/
def rev (s : List ℤ) : List ℤ := s.foldl (fun acc x => x :: acc) []

/-- SCHEME (fold): the reversal is a left fold with the reversed-prefix accumulator. -/
theorem cls : IsFold (fun s : List ℤ => s.foldl (fun acc x => x :: acc) []) :=
  ⟨(fun acc x => x :: acc), [], fun _ => rfl⟩

/-- The fold accumulates the reversed prefix in front of its seed — genuine incremental reversal. -/
theorem foldl_rev (s acc : List ℤ) : s.foldl (fun a x => x :: a) acc = s.reverse ++ acc := by
  induction s generalizing acc with
  | nil => simp
  | cons x xs ih =>
    simp only [List.foldl_cons, List.reverse_cons, ih, List.append_assoc, List.cons_append,
      List.nil_append]

/-- NON-VACUITY: the fold genuinely reverses the list, so `s = rev s` is the real palindrome test. -/
theorem corr (s : List ℤ) : rev s = s.reverse := by
  simp only [rev, foldl_rev, List.append_nil]

end LC.P0234
