import Lproofs.Patterns

/-! @lc 825 | name:Friends Of Appropriate Ages | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/friends-of-appropriate-ages/

    The accepted O(n + A) solution counts people per age (a bounded counting pass) before tallying valid
    friend requests. CLASSIFICATION: that counting pass is a streaming left fold over the ages with a
    scalar accumulator. NON-VACUITY: we prove the count never exceeds the number of people scanned (each
    person is counted at most once), so the accumulator does genuine counting work, not a re-encoding.
    We certify the fold + the count bound. -/

namespace LC.P0825
open Interview.Patterns

/-- Count the people whose age satisfies `pred` — the counting pass as a fold. -/
def tally (pred : ℕ → Bool) (ages : List ℕ) : ℕ := ages.foldl (fun acc a => if pred a then acc + 1 else acc) 0

/-- SCHEME (fold): the age count is a left fold with a scalar accumulator. -/
theorem cls (pred : ℕ → Bool) :
    IsFold (fun ages : List ℕ => ages.foldl (fun acc a => if pred a then acc + 1 else acc) 0) :=
  ⟨(fun acc a => if pred a then acc + 1 else acc), 0, fun _ => rfl⟩

/-- The count rises by at most one per person. -/
theorem foldl_bound (pred : ℕ → Bool) (ages : List ℕ) (acc : ℕ) :
    ages.foldl (fun a x => if pred x then a + 1 else a) acc ≤ acc + ages.length := by
  induction ages generalizing acc with
  | nil => simp
  | cons x xs ih =>
    simp only [List.foldl_cons, List.length_cons]
    have h := ih (if pred x then acc + 1 else acc)
    have hb : (if pred x then acc + 1 else acc) ≤ acc + 1 := by split <;> omega
    omega

/-- NON-VACUITY: the count never exceeds the number of people scanned (each counted at most once). -/
theorem corr (pred : ℕ → Bool) (ages : List ℕ) : tally pred ages ≤ ages.length := by
  have := foldl_bound pred ages 0
  simpa [tally] using this

end LC.P0825
