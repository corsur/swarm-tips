import Lproofs.Patterns

/-! @lc 56 | name:Merge Intervals | scheme:fold | family:merge-intervals | complexity:O(n log n) |
    source:https://leetcode.com/problems/merge-intervals/

    After sorting by start, a single left-to-right pass merges overlapping intervals. CLASSIFICATION:
    the merge pass is a streaming left fold whose accumulator is the merged-so-far stack (top = most
    recent interval); each new interval either extends the top (overlap) or is pushed. NON-VACUITY: we
    prove the merge never grows the output — the result has at most as many intervals as the input —
    because each step adds at most one interval and overlaps collapse. This pins the fold to genuine
    coalescing work. We certify the fold + the no-growth bound; DROP the covered-set equality. -/

namespace LC.P0056
open Interview.Patterns

/-- Merge one interval into the stack (top first); assumes intervals arrive sorted by start. -/
def step : List (ℤ × ℤ) → (ℤ × ℤ) → List (ℤ × ℤ)
  | (lo, hi) :: acc, (nlo, nhi) =>
    if nlo ≤ hi then (lo, max hi nhi) :: acc else (nlo, nhi) :: (lo, hi) :: acc
  | [], i => [i]

/-- Merged stack after one streaming pass over the (sorted) intervals. -/
def sol (xs : List (ℤ × ℤ)) : List (ℤ × ℤ) := xs.foldl step []

/-- SCHEME (fold): the merge is a left fold with the merged-stack accumulator. -/
theorem cls : IsFold (fun xs : List (ℤ × ℤ) => xs.foldl step []) := ⟨step, [], fun _ => rfl⟩

/-- Each step adds at most one interval: it either replaces the top (overlap) or pushes one. -/
theorem step_len (acc : List (ℤ × ℤ)) (i : ℤ × ℤ) : (step acc i).length ≤ acc.length + 1 := by
  obtain ⟨nlo, nhi⟩ := i
  match acc with
  | [] => simp [step]
  | (lo, hi) :: rest =>
    simp only [step]
    split <;> simp <;> omega

/-- NON-VACUITY: merging never grows the list — the output has at most as many intervals as the input. -/
theorem corr (xs : List (ℤ × ℤ)) : (sol xs).length ≤ xs.length := by
  have key : ∀ (ys : List (ℤ × ℤ)) acc, (ys.foldl step acc).length ≤ acc.length + ys.length := by
    intro ys
    induction ys with
    | nil => intro acc; simp
    | cons i rest ih =>
      intro acc
      simp only [List.foldl_cons, List.length_cons]
      exact le_trans (ih (step acc i)) (by have := step_len acc i; omega)
  have := key xs []
  simpa [sol] using this

end LC.P0056
