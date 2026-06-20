import Lproofs.Schemes.Fold

/-! @lc 2423 | name:Remove Letter To Equalize Frequency | scheme:fold | family:hashing |
    complexity:O(n) | source:https://leetcode.com/problems/remove-letter-to-equalize-frequency/

    We test whether removing a single occurrence of some letter equalises all letter frequencies; the
    frequencies are tallied in one pass. CLASSIFICATION (fold): per-letter counts are a streaming tally.
    CORRECTNESS: we certify the frequency-update the check relies on — erasing one occurrence of letter
    `c` leaves every OTHER letter's count unchanged, so only `c`'s frequency drops by one. -/

namespace LC.P2423
open Interview.Patterns

/-- SCHEME (fold): each letter's count is a streaming right-fold tally. -/
theorem cls (c : ℤ) : IsRightFold (fun L : List ℤ => L.countP fun x => decide (x = c)) := by
  refine ⟨fun x n => if decide (x = c) then n + 1 else n, 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons x t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- CORRECT: erasing one occurrence of `c` leaves every other letter's count untouched. -/
theorem corr (l : List ℤ) (c d : ℤ) (h : d ≠ c) : (l.erase c).count d = l.count d := by
  rw [List.count_erase_of_ne h]

end LC.P2423
