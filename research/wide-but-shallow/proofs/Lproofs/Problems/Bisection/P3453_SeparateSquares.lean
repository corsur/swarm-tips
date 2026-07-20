import Lproofs.Schemes.Bisect

/-! @lc 3453 | name:Separate Squares I | scheme:bisection | family:binary-search | complexity:O(n log) |
    source:https://leetcode.com/problems/separate-squares-i/

    Find the lowest horizontal line at height `Y` with at least half the total square area below it. The
    accepted solution binary-searches `Y`. CLASSIFICATION (bisection): the test ``at least half the area
    is below height `Y`'' is a monotone threshold over the CONCRETE area-below function of the actual
    squares (bottoms `b`, sides `s`), not a free predicate --- raising the line never lowers the area
    below. CORRECTNESS: the binary-search answer is the least height meeting the half-area condition. -/

namespace LC.P3453
open Interview.Patterns

/-- Area of the `m` concrete squares lying below height `Y`: each square (bottom `b i`, side `s i`)
    contributes `s i · min(s i, Y - b i)` (truncated subtraction clamps a line below the square to 0). -/
def areaBelow (b s : ℕ → ℕ) (m Y : ℕ) : ℕ :=
  (Finset.range m).sum (fun i => s i * min (s i) (Y - b i))

/-- The test the binary search uses: at least half the total area is below height `Y`. -/
def enough (b s : ℕ → ℕ) (m total Y : ℕ) : Prop := total ≤ 2 * areaBelow b s m Y

instance (b s : ℕ → ℕ) (m total : ℕ) : DecidablePred (enough b s m total) :=
  fun Y => inferInstanceAs (Decidable (total ≤ 2 * areaBelow b s m Y))

/-- The binary-search answer: the least height with at least half the area below. -/
def sol (b s : ℕ → ℕ) (m total : ℕ) (h : ∃ Y, enough b s m total Y) : ℕ := Nat.find h

/-- The area below is monotone in the line height (over the concrete squares). -/
theorem areaBelow_mono (b s : ℕ → ℕ) (m : ℕ) : Monotone (areaBelow b s m) := by
  intro a c hac
  apply Finset.sum_le_sum
  intro i _
  exact Nat.mul_le_mul_left _ (min_le_min_left _ (Nat.sub_le_sub_right hac _))

theorem enough_mono (b s : ℕ → ℕ) (m total : ℕ) :
    ∀ a c, a ≤ c → enough b s m total a → enough b s m total c :=
  fun _ _ hac ha => le_trans ha (Nat.mul_le_mul_left 2 (areaBelow_mono b s m hac))

/-- SCHEME (bisection): the half-area test is a monotone threshold — `enough Y ↔ answer ≤ Y`. -/
theorem cls (b s : ℕ → ℕ) (m total : ℕ) (h : ∃ Y, enough b s m total Y) (n : ℕ) :
    enough b s m total n ↔ sol b s m total h ≤ n :=
  bisection_threshold (enough b s m total) (enough_mono b s m total) h n

/-- CORRECT: the answer is the least height with at least half the concrete square area below it. -/
theorem corr (b s : ℕ → ℕ) (m total : ℕ) (h : ∃ Y, enough b s m total Y) :
    IsLeast {Y | enough b s m total Y} (sol b s m total h) :=
  bisection_isLeast (enough b s m total) h


/-- TEST VECTOR (official example 1, coordinates doubled to stay integral): one square at
    (0,0) with side 2 — total area 4, and the least height with half the area below is 1
    (the judge's 0.5, scaled by 2). -/
theorem vec :
    sol (fun _ => 0) (fun _ => 2) 1 4 ⟨1, by decide⟩ = 1 := by
  simp only [sol]
  rw [Nat.find_eq_iff]
  decide

end LC.P3453
