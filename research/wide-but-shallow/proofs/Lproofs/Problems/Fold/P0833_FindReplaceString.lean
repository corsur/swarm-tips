import Lproofs.Schemes.Fold

/-! @lc 833 | name:Find And Replace in String | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/find-and-replace-in-string/

    Scan `s` left to right; at each position emit either the matched replacement or the original
    character (the per-position emission `f c`). CLASSIFICATION: the output is a streaming left fold that
    appends `f c`. CORRECTNESS: we prove the scan computes exactly the per-position replacement — its
    output is `s.flatMap f`, the concatenation of every position's emission (so with `f = singleton` it
    reproduces the input, the no-replacement case). -/

namespace LC.P0833
open Interview.Patterns

/-- The output-building scan: fold the per-position emitter `f` over the string. -/
def scan (f : Char → List Char) (s : List Char) : List Char := s.foldl (fun out c => out ++ f c) []

/-- SCHEME (fold): the output scan is a streaming left fold. -/
theorem cls (f : Char → List Char) : IsFold (scan f) := ⟨fun out c => out ++ f c, [], fun _ => rfl⟩

/-- CORRECT: the scan's output is exactly the per-position replacement — every position's emission
    `f c` concatenated in order (`s.flatMap f`). -/
theorem corr (f : Char → List Char) (s : List Char) : scan f s = s.flatMap f := by
  have h : ∀ (l acc : List Char),
      l.foldl (fun out c => out ++ f c) acc = acc ++ l.flatMap f := by
    intro l
    induction l with
    | nil => intro acc; simp
    | cons c t ih => intro acc; simp [List.foldl_cons, ih, List.flatMap_cons, List.append_assoc]
  show s.foldl (fun out c => out ++ f c) [] = s.flatMap f
  rw [h s []]; simp

/-- The no-replacement case (each position emits its own character) reproduces the input. -/
theorem corr_id (s : List Char) : scan (fun c => [c]) s = s := by
  rw [corr]; simp

end LC.P0833
