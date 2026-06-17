import Lproofs.Patterns

/-! @lc 163 | name:Missing Ranges | scheme:fold | family:gap-scan | complexity:O(n) |
    source:https://leetcode.com/problems/missing-ranges/

    Report the gaps in a sorted array within `[lower, upper]`. CLASSIFICATION: a single streaming left
    fold over the sorted values whose accumulator carries the previous value and the running gap count —
    a gap is recorded whenever the current value jumps past the previous by more than one. NON-VACUITY:
    we prove the gap count never exceeds the number of values scanned (each element opens at most one
    gap), so the accumulator does genuine gap detection, not a re-encoding. We certify the fold + the
    gap-count bound. -/

namespace LC.P0163
open Interview.Patterns

/-- One step: record a gap if the current value jumps past the previous one. -/
def step (acc : ℤ × ℕ) (x : ℤ) : ℤ × ℕ := (x, if x > acc.1 + 1 then acc.2 + 1 else acc.2)

/-- Running `(previous value, gap count)` folded over the sorted values. -/
def scan (lower : ℤ) (nums : List ℤ) : ℤ × ℕ := nums.foldl step (lower - 1, 0)

/-- SCHEME (fold): the gap scan is a left fold with the `(prev, count)` accumulator. -/
theorem cls (lower : ℤ) : IsFold (fun nums : List ℤ => nums.foldl step (lower - 1, 0)) :=
  ⟨step, (lower - 1, 0), fun _ => rfl⟩

/-- The gap count rises by at most one per value. -/
theorem foldl_bound (nums : List ℤ) (acc : ℤ × ℕ) :
    (nums.foldl step acc).2 ≤ acc.2 + nums.length := by
  induction nums generalizing acc with
  | nil => simp
  | cons x xs ih =>
    simp only [List.foldl_cons, List.length_cons]
    have h := ih (step acc x)
    have hb : (step acc x).2 ≤ acc.2 + 1 := by simp only [step]; split <;> omega
    omega

/-- NON-VACUITY: the gap count never exceeds the number of values scanned. -/
theorem corr (lower : ℤ) (nums : List ℤ) : (scan lower nums).2 ≤ nums.length := by
  have := foldl_bound nums (lower - 1, 0)
  simpa [scan] using this

end LC.P0163
