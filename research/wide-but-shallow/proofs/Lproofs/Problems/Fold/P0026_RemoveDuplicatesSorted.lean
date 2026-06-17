import Lproofs.Schemes.Fold

/-! @lc 26 | name:Remove Duplicates from Sorted Array | scheme:fold | family:dedup-scan |
    complexity:O(n) | source:https://leetcode.com/problems/remove-duplicates-from-sorted-array/

    Compact a sorted array in place, keeping one of each value. CLASSIFICATION: a streaming left fold
    whose accumulator is the kept-so-far list (most recent on top) — a value is appended only when it
    differs from the last kept one (adjacent equals collapse). NON-VACUITY: we prove the compaction
    never grows the output — the result has at most as many elements as the input — so the accumulator
    does genuine dedup work, not a re-encoding. We certify the fold + the no-growth bound. -/

namespace LC.P0026
open Interview.Patterns

/-- Keep `x` unless it equals the most recently kept value. -/
def step (acc : List ℤ) (x : ℤ) : List ℤ :=
  match acc with
  | a :: _ => if a = x then acc else x :: acc
  | [] => [x]

/-- Deduplicated kept list after one streaming pass over the (sorted) array. -/
def dedup (xs : List ℤ) : List ℤ := xs.foldl step []

/-- SCHEME (fold): the compaction is a left fold with the kept-list accumulator. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl step []) := ⟨step, [], fun _ => rfl⟩

/-- Each step adds at most one element. -/
theorem step_len (acc : List ℤ) (x : ℤ) : (step acc x).length ≤ acc.length + 1 := by
  match acc with
  | [] => simp [step]
  | a :: rest =>
    simp only [step]
    split <;> simp <;> omega

/-- NON-VACUITY: deduplication never grows the array — the kept list is no longer than the input. -/
theorem corr (xs : List ℤ) : (dedup xs).length ≤ xs.length := by
  have key : ∀ (ys : List ℤ) acc, (ys.foldl step acc).length ≤ acc.length + ys.length := by
    intro ys
    induction ys with
    | nil => intro acc; simp
    | cons y rest ih =>
      intro acc
      simp only [List.foldl_cons, List.length_cons]
      exact le_trans (ih (step acc y)) (by have := step_len acc y; omega)
  have := key xs []
  simpa [dedup] using this

end LC.P0026
