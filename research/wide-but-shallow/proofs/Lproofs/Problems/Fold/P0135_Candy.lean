import Lproofs.Patterns

/-! @lc 135 | name:Candy | scheme:fold | family:running-scan | complexity:O(n) |
    source:https://leetcode.com/problems/candy/

    Each child gets at least one candy, and more than a lower-rated neighbour; the left-to-right pass
    gives child `i` one more than the previous if its rating is higher, else one. CLASSIFICATION: that
    pass is a streaming left fold whose accumulator carries the previous rating, the previous candy
    count, and the running total. NON-VACUITY: we prove the total is at least the number of children —
    every child receives at least one candy — so the accumulator does genuine candy-assignment work,
    not a re-encoding. We certify the fold + the ≥ one-per-child bound. -/

namespace LC.P0135
open Interview.Patterns

/-- One step of the left-to-right pass: assign this child its candy and accumulate the total. -/
def step (s : ℕ × ℕ × ℕ) (r : ℕ) : ℕ × ℕ × ℕ :=
  let c := if r > s.1 then s.2.1 + 1 else 1
  (r, c, s.2.2 + c)

/-- Running `(prev rating, prev candy, total)` folded over the ratings. -/
def total (ratings : List ℕ) : ℕ := (ratings.foldl step (0, 0, 0)).2.2

/-- SCHEME (fold): the candy pass is a left fold with the `(prevRating, prevCandy, total)` accumulator. -/
theorem cls : IsFold (fun ratings : List ℕ => ratings.foldl step (0, 0, 0)) :=
  ⟨step, (0, 0, 0), fun _ => rfl⟩

/-- Each step adds at least one candy to the total. -/
theorem step_ge (s : ℕ × ℕ × ℕ) (r : ℕ) : (step s r).2.2 ≥ s.2.2 + 1 := by
  simp only [step]; split <;> omega

/-- NON-VACUITY: the total is at least the number of children — everyone gets at least one candy. -/
theorem corr (ratings : List ℕ) : total ratings ≥ ratings.length := by
  have key : ∀ (rs : List ℕ) s, (rs.foldl step s).2.2 ≥ s.2.2 + rs.length := by
    intro rs
    induction rs with
    | nil => intro s; simp
    | cons r rest ih =>
      intro s
      simp only [List.foldl_cons, List.length_cons]
      have h := ih (step s r)
      have hs := step_ge s r
      omega
  have := key ratings (0, 0, 0)
  simpa [total] using this

end LC.P0135
