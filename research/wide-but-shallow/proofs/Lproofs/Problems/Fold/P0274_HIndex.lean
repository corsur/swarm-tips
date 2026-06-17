import Lproofs.Patterns

/-! @lc 274 | name:H-Index | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/h-index/

    The accepted O(n) solution counts, by a counting-sort pass, how many papers have at least `h`
    citations. CLASSIFICATION: that count is a streaming left fold over the citations with a scalar
    accumulator. NON-VACUITY: we prove the count is antitone in the threshold — raising `h` can only
    reduce how many papers qualify — which is exactly the monotone structure the h-index search rides on.
    We certify the fold + the antitone-count property. -/

namespace LC.P0274
open Interview.Patterns

/-- Count papers with at least `h` citations — the counting-sort tally as a fold. -/
def atLeast (h : ℕ) (cites : List ℕ) : ℕ := cites.foldl (fun acc c => if h ≤ c then acc + 1 else acc) 0

/-- SCHEME (fold): the at-least-`h` count is a left fold with a scalar accumulator. -/
theorem cls (h : ℕ) : IsFold (fun cites : List ℕ => cites.foldl (fun acc c => if h ≤ c then acc + 1 else acc) 0) :=
  ⟨(fun acc c => if h ≤ c then acc + 1 else acc), 0, fun _ => rfl⟩

/-- A larger threshold qualifies no more papers at every accumulator value. -/
theorem foldl_antitone (h1 h2 : ℕ) (hh : h1 ≤ h2) : ∀ (cites : List ℕ) (a b : ℕ), a ≤ b →
    cites.foldl (fun acc c => if h2 ≤ c then acc + 1 else acc) a
      ≤ cites.foldl (fun acc c => if h1 ≤ c then acc + 1 else acc) b := by
  intro cites
  induction cites with
  | nil => intro a b hab; simpa using hab
  | cons c cs ih =>
    intro a b hab
    simp only [List.foldl_cons]
    apply ih
    by_cases h2c : h2 ≤ c
    · have h1c : h1 ≤ c := le_trans hh h2c
      simp only [if_pos h2c, if_pos h1c]; omega
    · simp only [if_neg h2c]; split <;> omega

/-- NON-VACUITY: the at-least count is antitone in the threshold `h` — the monotone structure the
    h-index search exploits. -/
theorem corr (cites : List ℕ) (h1 h2 : ℕ) (hh : h1 ≤ h2) : atLeast h2 cites ≤ atLeast h1 cites := by
  exact foldl_antitone h1 h2 hh cites 0 0 (le_refl 0)

end LC.P0274
