import Lproofs.Schemes.Fold

/-! @lc 2423 | name:Remove Letter To Equalize Frequency | scheme:fold | family:hashing |
    complexity:O(n) | source:https://leetcode.com/problems/remove-letter-to-equalize-frequency/

    We test whether removing a single occurrence of some letter equalises all letter frequencies; the
    frequencies are tallied in one pass. CLASSIFICATION (fold): per-letter counts are a streaming tally.
    CORRECTNESS: we certify the frequency-update the check relies on — erasing one occurrence of letter
    `c` leaves every OTHER letter's count unchanged, so only `c`'s frequency drops by one. -/

namespace LC.P2423
open Interview.Patterns

/-- The per-letter tally the frequency check reads. -/
def sol (c : ℤ) (L : List ℤ) : ℕ := L.count c

/-- SCHEME (fold): each letter's count `sol c` is a streaming right-fold tally. -/
theorem cls (c : ℤ) : IsRightFold (sol c) := by
  refine ⟨fun x n => if x == c then n + 1 else n, 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons x t ih =>
    simp only [sol, List.count_cons, List.foldr_cons] at *
    rw [← ih]
    split <;> omega

/-- CORRECT: erasing one occurrence of `c` leaves every other letter's tally untouched — deleting
    one letter only moves that letter's frequency. -/
theorem corr (l : List ℤ) (c d : ℤ) (h : d ≠ c) : sol d (l.erase c) = sol d l := by
  simp only [sol]
  rw [List.count_erase_of_ne h]


/-- GROUND INSTANCE (official example 1, "abcc" as 1,2,3,3): c tallies 2; erasing one c drops it
    to 1, equal to every other letter's tally. -/
theorem vec : sol 3 [1, 2, 3, 3] = 2 ∧ sol 3 ([1, 2, 3, 3].erase 3) = 1 ∧
    sol 1 [1, 2, 3, 3] = 1 := by decide

end LC.P2423
