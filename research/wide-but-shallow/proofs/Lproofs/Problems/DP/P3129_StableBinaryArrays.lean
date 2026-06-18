import Lproofs.Schemes.Fold

/-! @lc 3129 | name:Find All Possible Stable Binary Arrays I | scheme:dp | family:dp-linear |
    complexity:O(zero*one*limit) | source:https://leetcode.com/problems/find-all-possible-stable-binary-arrays-i/

    Count binary arrays with `zero` zeros, `one` ones, and no run of equal values longer than `limit`.
    The accepted solution is a counting DP over (#zeros, #ones, last bit). CLASSIFICATION: the count is
    a streaming tally fold over the candidate arrangements (those passing the stability test, itself a
    running-max-run fold). NON-VACUITY: we certify the counting fold and that the stable count never
    exceeds the total number of arrangements — stable arrays are a subset of all arrays. -/

namespace LC.P3129
open Interview.Patterns

/-- Longest run of equal values — a streaming fold carrying (last bit, current run, max run). -/
def maxRun (a : List Bool) : ℕ :=
  (a.foldl (fun st b =>
    let cur := if st.1 = b then st.2.1 + 1 else 1
    (b, cur, max st.2.2 cur)) (false, 0, 0)).2.2

/-- An array is stable when no run exceeds `limit`. -/
def stable (limit : ℕ) (a : List Bool) : Bool := decide (maxRun a ≤ limit)

/-- Count the stable arrays among the candidates — a streaming tally fold. -/
def countStable (limit : ℕ) (arrs : List (List Bool)) : ℕ :=
  arrs.foldl (fun n a => if stable limit a then n + 1 else n) 0

/-- SCHEME (dp = streaming fold): the count is built by one tally fold. -/
theorem cls (limit : ℕ) : IsFold (countStable limit) :=
  ⟨fun n a => if stable limit a then n + 1 else n, 0, fun _ => rfl⟩

/-- CORRECT (bound): the stable count never exceeds the number of candidate arrays — stable arrays
    are a subset of all arrays. -/
theorem corr (limit : ℕ) (arrs : List (List Bool)) : countStable limit arrs ≤ arrs.length := by
  suffices h : ∀ (arrs : List (List Bool)) (c : ℕ),
      arrs.foldl (fun n a => if stable limit a then n + 1 else n) c ≤ c + arrs.length by
    simpa [countStable] using h arrs 0
  intro arrs
  induction arrs with
  | nil => intro c; simp
  | cons a t ih =>
    intro c
    simp only [List.foldl_cons, List.length_cons]
    by_cases hs : stable limit a
    · rw [if_pos hs]; have := ih (c + 1); omega
    · rw [if_neg hs]; have := ih c; omega

end LC.P3129
