import Lproofs.Schemes.Fold

/-! @lc 38 | name:Count and Say | scheme:fold | family:string-other | complexity:O(n) |
    source:https://leetcode.com/problems/count-and-say/

    Each term is the run-length "saying" of the previous one (count, then character, per maximal
    run). Correctness of the core `say` operation: expanding the run-length encoding recovers the
    original string — the encoding is lossless. (The decimal rendering of counts is a presentation
    step on top of this grouping; the nth term is `say` iterated.) -/

namespace LC.P0038
open Interview.Patterns

/-- Run-length "say": maximal runs as `(count, char)` pairs. -/
def say : List Char → List (ℕ × Char)
  | [] => []
  | c :: rest =>
    match say rest with
    | (n, d) :: tl => if c = d then (n + 1, d) :: tl else (1, c) :: (n, d) :: tl
    | [] => [(1, c)]

/-- Expand a run-length encoding back into characters. -/
def expand : List (ℕ × Char) → List Char
  | [] => []
  | (n, c) :: tl => List.replicate n c ++ expand tl

def sol (s : List Char) : List (ℕ × Char) := say s

/-- SCHEME (fold): `say` is a genuine list fold — a `foldr` whose combiner extends or opens the head
    run of the accumulated run-length saying. This is exactly `sol`, not a proxy. -/
theorem cls : IsRightFold sol :=
  ⟨fun c acc => match acc with
      | (n, d) :: tl => if c = d then (n + 1, d) :: tl else (1, c) :: (n, d) :: tl
      | [] => [(1, c)],
   [], by
    intro s
    change say s = _
    induction s with
    | nil => rfl
    | cons c rest ih => simp only [say, List.foldr_cons, ih]⟩

/-- CORRECT (lossless): expanding the run-length saying recovers the original string. -/
theorem corr (s : List Char) : expand (sol s) = s := by
  unfold sol
  induction s with
  | nil => rfl
  | cons c rest ih =>
    simp only [say]
    cases hr : say rest with
    | nil =>
      simp only [hr, expand] at ih
      simp [expand, List.replicate, ← ih]
    | cons hd tl =>
      obtain ⟨n, d⟩ := hd
      simp only [hr, expand] at ih
      by_cases hcd : c = d
      · simp only [if_pos hcd]
        subst hcd
        simp only [expand, List.replicate_succ, List.cons_append, ih]
      · simp only [if_neg hcd, expand, List.replicate_one, List.singleton_append, ih]

end LC.P0038
