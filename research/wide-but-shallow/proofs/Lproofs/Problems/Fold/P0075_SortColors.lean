import Lproofs.Patterns

/-! @lc 75 | name:Sort Colors | scheme:fold | family:counting | complexity:O(n) |
    source:https://leetcode.com/problems/sort-colors/

    Dutch-national-flag: sort an array of 0/1/2 in one pass. CLASSIFICATION: the counting form of the
    algorithm is a streaming left fold whose accumulator is the triple of running tallies `(z, o, t)`
    of 0s/1s/2s seen; the sorted output is `0^z ++ 1^o ++ 2^t`. NON-VACUITY: we prove the three tallies
    always sum to the number of elements consumed — every element is counted exactly once, so the
    accumulator tracks genuine partition state (not a re-encoding). We certify the fold + the
    count-conservation invariant. -/

namespace LC.P0075
open Interview.Patterns

/-- One counting step: bump the bucket of the seen colour (anything ≥ 2 lands in the 2-bucket). -/
def step : (ℕ × ℕ × ℕ) → ℕ → (ℕ × ℕ × ℕ)
  | (z, o, t), c => if c = 0 then (z + 1, o, t) else if c = 1 then (z, o + 1, t) else (z, o, t + 1)

/-- Running tallies of the colours, accumulated left to right. -/
def counts (xs : List ℕ) : ℕ × ℕ × ℕ := xs.foldl step (0, 0, 0)

/-- SCHEME (fold): the colour tallies are a left fold with a triple accumulator. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl step (0, 0, 0)) := ⟨step, (0, 0, 0), fun _ => rfl⟩

/-- Each step bumps exactly one bucket: the tally sum rises by one. -/
theorem step_sum (acc : ℕ × ℕ × ℕ) (c : ℕ) :
    (step acc c).1 + (step acc c).2.1 + (step acc c).2.2 = acc.1 + acc.2.1 + acc.2.2 + 1 := by
  obtain ⟨z, o, t⟩ := acc
  simp only [step]
  split <;> [skip; split] <;> simp <;> omega

/-- The running tallies always sum to the count of elements consumed. -/
theorem foldl_sum (xs : List ℕ) (acc : ℕ × ℕ × ℕ) :
    let r := xs.foldl step acc
    r.1 + r.2.1 + r.2.2 = acc.1 + acc.2.1 + acc.2.2 + xs.length := by
  induction xs generalizing acc with
  | nil => simp
  | cons c xs ih =>
    simp only [List.foldl_cons, List.length_cons]
    rw [ih (step acc c), step_sum acc c]
    omega

/-- NON-VACUITY: the three colour tallies sum to the array length — every element counted once. -/
theorem corr (xs : List ℕ) : (counts xs).1 + (counts xs).2.1 + (counts xs).2.2 = xs.length := by
  have h := foldl_sum xs (0, 0, 0)
  simpa [counts] using h

end LC.P0075
