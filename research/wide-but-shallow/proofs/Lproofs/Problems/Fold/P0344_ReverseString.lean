import Lproofs.Patterns

/-! @lc 344 | name:Reverse String | scheme:fold | family:list-reverse | complexity:O(n) |
    source:https://leetcode.com/problems/reverse-string/

    Reverse a character array. CLASSIFICATION: the reversal is a streaming left fold whose accumulator
    is the reversed-so-far prefix — each character is consed onto the front of the accumulator.
    NON-VACUITY: we prove the fold equals the genuine `List.reverse`, so the accumulator carries real
    reversed state, not a re-encoding. We certify the fold + the reversal correctness. -/

namespace LC.P0344
open Interview.Patterns

/-- Reverse by cons-folding onto the accumulator. -/
def rev (s : List Char) : List Char := s.foldl (fun acc c => c :: acc) []

/-- SCHEME (fold): the reversal is a left fold with the reversed-prefix accumulator. -/
theorem cls : IsFold (fun s : List Char => s.foldl (fun acc c => c :: acc) []) :=
  ⟨(fun acc c => c :: acc), [], fun _ => rfl⟩

/-- The fold accumulates the reversed prefix in front of its seed — genuine incremental reversal. -/
theorem foldl_rev (s acc : List Char) :
    s.foldl (fun a c => c :: a) acc = s.reverse ++ acc := by
  induction s generalizing acc with
  | nil => simp
  | cons c cs ih => simp only [List.foldl_cons, List.reverse_cons, ih, List.append_assoc, List.cons_append, List.nil_append]

/-- NON-VACUITY: the fold genuinely reverses the string. -/
theorem corr (s : List Char) : rev s = s.reverse := by
  simp only [rev, foldl_rev, List.append_nil]

end LC.P0344
