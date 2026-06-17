import Lproofs.Patterns

/-! @lc 485 | name:Max Consecutive Ones | scheme:fold | family:running-counter | complexity:O(n) |
    source:https://leetcode.com/problems/max-consecutive-ones/

    Longest run of consecutive `1`s. CLASSIFICATION: a single streaming left fold whose accumulator is
    the pair `(current run length, best so far)` — a `1` extends the run and bumps the best, anything
    else resets the run. NON-VACUITY: we prove the invariant the answer relies on — the best is always
    at least the current run — so the accumulator carries genuine running-counter state, not a
    re-encoding. We certify the fold + the best ≥ current invariant. -/

namespace LC.P0485
open Interview.Patterns

/-- One step: extend the run on a `1` (raising the best), reset it otherwise. -/
def step : (ℕ × ℕ) → ℕ → (ℕ × ℕ)
  | (cur, best), x => if x = 1 then (cur + 1, max best (cur + 1)) else (0, best)

/-- Running `(current run, best)` accumulated left to right. -/
def scan (xs : List ℕ) : ℕ × ℕ := xs.foldl step (0, 0)

/-- SCHEME (fold): the scan is a left fold with the `(current, best)` accumulator. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl step (0, 0)) := ⟨step, (0, 0), fun _ => rfl⟩

/-- Each step keeps the invariant `best ≥ current`. -/
theorem step_inv (acc : ℕ × ℕ) (x : ℕ) (h : acc.2 ≥ acc.1) : (step acc x).2 ≥ (step acc x).1 := by
  obtain ⟨cur, best⟩ := acc
  simp only [step]
  split
  · exact le_max_right _ _
  · exact Nat.zero_le _

/-- NON-VACUITY: the best run is always at least the current run — the running-counter invariant. -/
theorem corr (xs : List ℕ) : (scan xs).2 ≥ (scan xs).1 := by
  have key : ∀ (ys : List ℕ) acc, acc.2 ≥ acc.1 → (ys.foldl step acc).2 ≥ (ys.foldl step acc).1 := by
    intro ys
    induction ys with
    | nil => intro acc h; exact h
    | cons y rest ih => intro acc h; exact ih (step acc y) (step_inv acc y h)
  exact key xs (0, 0) (le_refl 0)

end LC.P0485
