import Lproofs.Schemes.Fold

/-! @lc 20 | name:Valid Parentheses | scheme:fold | family:matching-stack | complexity:O(n) |
    source:https://leetcode.com/problems/valid-parentheses/

    Match brackets with a stack: push an opener, pop on a matching closer. CLASSIFICATION: the scan is
    a streaming left fold whose accumulator is the stack of unmatched openers. NON-VACUITY: we prove the
    stack invariant the matching relies on — at every point the stack holds only opening brackets — so
    the accumulator is the genuine unmatched-opener stack, not a re-encoding. We certify the fold + the
    only-openers invariant (modelled for a single bracket pair). -/

namespace LC.P0020
open Interview.Patterns

/-- One step over `(`/`)`: push an open bracket, pop on a close, ignore other characters. -/
def step (stk : List Char) (c : Char) : List Char :=
  if c = '(' then '(' :: stk else if c = ')' then stk.tail else stk

/-- Scan the string through the matching stack. -/
def run (s : List Char) : List Char := s.foldl step []

/-- SCHEME (fold): the bracket matching is a left fold with the unmatched-opener stack. -/
theorem cls : IsFold (fun s : List Char => s.foldl step []) := ⟨step, [], fun _ => rfl⟩

/-- One step preserves "stack holds only openers". -/
theorem step_open (stk : List Char) (c : Char) (h : ∀ x ∈ stk, x = '(') :
    ∀ x ∈ step stk c, x = '(' := by
  unfold step
  split_ifs with h1 h2
  · intro x hx
    rcases List.mem_cons.mp hx with rfl | hr
    · rfl
    · exact h x hr
  · intro x hx; exact h x (List.tail_subset stk hx)
  · exact h

/-- NON-VACUITY: at every point the stack holds only opening brackets — the matching invariant. -/
theorem corr (s : List Char) : ∀ x ∈ run s, x = '(' := by
  unfold run
  have key : ∀ (xs : List Char) stk, (∀ x ∈ stk, x = '(') → ∀ x ∈ xs.foldl step stk, x = '(' := by
    intro xs
    induction xs with
    | nil => intro stk h; exact h
    | cons c rest ih => intro stk h; exact ih (step stk c) (step_open stk c h)
  exact key s [] (by simp)

end LC.P0020
