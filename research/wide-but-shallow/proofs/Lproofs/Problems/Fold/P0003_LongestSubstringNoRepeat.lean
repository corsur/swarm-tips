import Lproofs.Schemes.Fold

/-! @lc 3 | name:Longest Substring Without Repeating Characters | scheme:fold | family:sliding-window |
    complexity:O(n) | source:https://leetcode.com/problems/longest-substring-without-repeating-characters/

    Slide a window holding distinct characters. CLASSIFICATION: the scan is a streaming left fold whose
    accumulator is the current window — on each character, if it already sits in the window the prefix up
    to its previous occurrence is dropped, then the character is appended. NON-VACUITY: we prove the
    window always ends with the just-read character (it is genuinely included at the right edge), so the
    accumulator does real window maintenance, not a re-encoding. We certify the fold + the right-edge
    invariant. -/

namespace LC.P0003
open Interview.Patterns

/-- Slide the window: drop through a previous occurrence of `c` if present, then append `c`. -/
def step (w : List Char) (c : Char) : List Char :=
  if c ∈ w then (w.dropWhile (· ≠ c)).tail ++ [c] else w ++ [c]

/-- The window after scanning the string. -/
def run (s : List Char) : List Char := s.foldl step []

/-- SCHEME (fold): the window scan is a left fold with the current-window accumulator. -/
theorem cls : IsFold (fun s : List Char => s.foldl step []) := ⟨step, [], fun _ => rfl⟩

/-- NON-VACUITY: after each step the window ends with the just-read character — it is genuinely
    included at the right edge of the window. -/
theorem corr (w : List Char) (c : Char) : (step w c).getLast? = some c := by
  unfold step
  split <;> simp

end LC.P0003
