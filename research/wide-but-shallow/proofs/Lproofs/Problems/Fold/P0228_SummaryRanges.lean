import Lproofs.Schemes.Fold

/-! @lc 228 | name:Summary Ranges | scheme:fold | family:two-pointers | complexity:O(n) |
    source:https://leetcode.com/problems/summary-ranges/

    Group a list into maximal runs of consecutive integers. This is the algorithmic core; the
    LeetCode variant renders each `(start, length)` run as the string `"lo->hi"` or `"lo"`, a
    presentation detail on top of the grouping. Correctness is a lossless round-trip: expanding
    the runs recovers the original list. -/

namespace LC.P0228
open Interview.Patterns

/-- The `len` consecutive integers starting at `s`: `[s, s+1, …, s+len-1]`. -/
def fromLen (s : ℤ) : ℕ → List ℤ
  | 0 => []
  | n + 1 => s :: fromLen (s + 1) n

/-- Group into maximal consecutive runs, each a `(start, length)` pair. -/
def ranges : List ℤ → List (ℤ × ℕ)
  | [] => []
  | x :: rest =>
    match ranges rest with
    | (s, len) :: tl => if x + 1 = s then (x, len + 1) :: tl else (x, 1) :: (s, len) :: tl
    | [] => [(x, 1)]

/-- Expand runs back into the underlying integers. -/
def expand : List (ℤ × ℕ) → List ℤ
  | [] => []
  | (s, len) :: tl => fromLen s len ++ expand tl

/-- The grouping returned to the caller. -/
def sol (xs : List ℤ) : List (ℤ × ℕ) := ranges xs

/-- SCHEME (fold): `ranges` is a genuine list fold — a `foldr` whose combiner extends or opens the
    head run of the accumulated grouping. This is exactly `sol`, not a proxy. -/
theorem cls : IsRightFold sol :=
  ⟨fun x acc => match acc with
      | (s, len) :: tl => if x + 1 = s then (x, len + 1) :: tl else (x, 1) :: (s, len) :: tl
      | [] => [(x, 1)],
   [], by
    intro xs
    change ranges xs = _
    induction xs with
    | nil => rfl
    | cons x rest ih => simp only [ranges, List.foldr_cons, ih]⟩

/-- CORRECT (lossless): expanding the consecutive-run grouping recovers the original list. -/
theorem corr (xs : List ℤ) : expand (sol xs) = xs := by
  unfold sol
  induction xs with
  | nil => rfl
  | cons x rest ih =>
    simp only [ranges]
    cases hr : ranges rest with
    | nil =>
      simp only [hr, expand] at ih
      simp [expand, fromLen, ← ih]
    | cons hd tl =>
      obtain ⟨s, len⟩ := hd
      simp only [hr, expand] at ih
      by_cases hx : x + 1 = s
      · simp only [if_pos hx]
        subst hx
        simp only [expand, fromLen, List.cons_append, ih]
      · simp only [if_neg hx]
        simp only [expand, fromLen, List.nil_append, List.cons_append, ih]

end LC.P0228
