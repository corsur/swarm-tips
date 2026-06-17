import Lproofs.Schemes.Fold

/-! @lc 833 | name:Find And Replace in String | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/find-and-replace-in-string/

    Scan `s` left to right; at each position emit either the matched replacement or the original
    character. CLASSIFICATION: the output is a streaming left fold that appends a per-position emission
    `f c`. NON-VACUITY: `cls` is the fold; `corr` proves that when no replacement applies (each position
    emits its own character) the scan reproduces the input exactly — the transform is the identity on
    the empty replacement set. -/

namespace LC.P0833
open Interview.Patterns

/-- The output-building scan: fold the per-position emitter `f` over the string. -/
def scan (f : Char → List Char) (s : List Char) : List Char := s.foldl (fun out c => out ++ f c) []

/-- SCHEME (fold): the output scan is a streaming left fold. -/
theorem cls (f : Char → List Char) : IsFold (scan f) := ⟨fun out c => out ++ f c, [], fun _ => rfl⟩

/-- CORRECT: with no replacements (each position emits its own character) the scan reproduces `s`. -/
theorem corr (s : List Char) : scan (fun c => [c]) s = s := by
  have h : ∀ (l : List Char) (acc : List Char),
      l.foldl (fun out c => out ++ [c]) acc = acc ++ l := by
    intro l
    induction l with
    | nil => intro acc; simp
    | cons c t ih => intro acc; simp [List.foldl_cons, ih, List.append_assoc]
  show s.foldl (fun out c => out ++ [c]) [] = s
  rw [h s []]; simp

end LC.P0833
