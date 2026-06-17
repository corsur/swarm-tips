import Lproofs.Schemes.Fold

/-! @lc 921 | name:Minimum Add to Make Parentheses Valid | scheme:fold | family:matching-stack |
    complexity:O(n) | source:https://leetcode.com/problems/minimum-add-to-make-parentheses-valid/

    Count the minimum insertions to balance a parenthesis string in one pass. CLASSIFICATION: a
    streaming left fold whose accumulator is the pair `(insertions needed, open balance)` — `(` raises
    the balance, `)` cancels an open if available else demands an insertion. NON-VACUITY: we prove the
    genuine cancellation recurrence — a `)` against a positive balance simply decrements it — and that
    the running state distinguishes unmatched closers from unmatched openers (e.g. `")("` needs one of
    each). We certify the fold + the matching recurrence. -/

namespace LC.P0921
open Interview.Patterns

/-- One step: `(` opens, `)` cancels an open or demands an insertion, other chars pass through. -/
def step : (ℕ × ℕ) → Char → (ℕ × ℕ)
  | (adds, open'), c =>
    if c = '(' then (adds, open' + 1)
    else if c = ')' then (if open' > 0 then (adds, open' - 1) else (adds + 1, open'))
    else (adds, open')

/-- Running `(insertions needed, open balance)` folded over the string. -/
def scan (s : List Char) : ℕ × ℕ := s.foldl step (0, 0)

/-- SCHEME (fold): the balance tracking is a left fold with the `(adds, open)` accumulator. -/
theorem cls : IsFold (fun s : List Char => s.foldl step (0, 0)) := ⟨step, (0, 0), fun _ => rfl⟩

/-- NON-VACUITY (matching recurrence): a `)` against a positive open balance just cancels one open. -/
theorem corr_match (adds open' : ℕ) : step (adds, open' + 1) ')' = (adds, open') := by
  simp [step]

/-- NON-VACUITY: the running state separates unmatched closers from unmatched openers — `")("` needs
    one insertion of each (final state `(1, 1)`, total `2`). -/
theorem corr : scan [')', '('] = (1, 1) := by decide

end LC.P0921
