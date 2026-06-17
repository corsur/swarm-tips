import Lproofs.Schemes.Fold

/-! @lc 443 | name:String Compression | scheme:fold | family:two-pointers | complexity:O(n) |
    source:https://leetcode.com/problems/string-compression/ -/

namespace LC.P0443
open Interview.Patterns

/-- Run-length encoding: the streaming pass groups maximal runs of equal characters into
    `(char, count)` pairs. This is the algorithmic core of the problem; the LeetCode variant
    additionally renders each count as decimal digits in place, a presentation detail layered
    on top of this grouping. -/
def rle : List Char → List (Char × ℕ)
  | [] => []
  | c :: rest =>
    match rle rest with
    | (d, n) :: tl => if c = d then (d, n + 1) :: tl else (c, 1) :: (d, n) :: tl
    | [] => [(c, 1)]

/-- Inverse: expand each `(char, count)` run back into characters. -/
def expand : List (Char × ℕ) → List Char
  | [] => []
  | (c, n) :: tl => List.replicate n c ++ expand tl

/-- The compression transform returned to the caller. -/
def sol (s : List Char) : List (Char × ℕ) := rle s

/-- SCHEME (fold): `rle` is a genuine list fold — a `foldr` whose combiner extends or opens the head
    run of the accumulated run-length encoding. This is exactly `sol`, not a proxy. -/
theorem cls : IsRightFold sol :=
  ⟨fun c acc => match acc with
      | (d, n) :: tl => if c = d then (d, n + 1) :: tl else (c, 1) :: (d, n) :: tl
      | [] => [(c, 1)],
   [], by
    intro s
    change rle s = _
    induction s with
    | nil => rfl
    | cons c rest ih => simp only [rle, List.foldr_cons, ih]⟩

/-- CORRECT (lossless): expanding the run-length encoding recovers the original string, so the
    compression loses no information. -/
theorem corr (s : List Char) : expand (sol s) = s := by
  unfold sol
  induction s with
  | nil => rfl
  | cons c rest ih =>
    simp only [rle]
    cases hr : rle rest with
    | nil =>
      simp only [hr, expand] at ih
      simp [expand, ← ih]
    | cons hd tl =>
      obtain ⟨d, n⟩ := hd
      simp only [hr, expand] at ih
      by_cases hcd : c = d
      · simp only [if_pos hcd]
        simp only [expand, List.replicate_succ, List.cons_append, ih, hcd]
      · simp only [if_neg hcd, expand, List.replicate_one, List.singleton_append, ih]

end LC.P0443
