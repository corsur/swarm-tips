import Lproofs.Schemes.Fold

/-! @lc 1047 | name:Remove All Adjacent Duplicates In String | scheme:fold | family:pairing-stack |
    complexity:O(n) | source:https://leetcode.com/problems/remove-all-adjacent-duplicates-in-string/

    Repeatedly remove adjacent equal-pairs; a stack resolves it in one pass. CLASSIFICATION: the
    resolution is a streaming left fold whose accumulator is the survivor stack — each character cancels
    with the top if equal, otherwise it is pushed. NON-VACUITY: we prove the survivor stack is *reduced*
    — no two adjacent equal characters remain — which is exactly the termination condition of the
    process. We certify the fold + the reducedness invariant. -/

namespace LC.P1047
open Interview.Patterns

/-- Resolve one character against the stack: cancel with an equal top, else push. -/
def step : List Char → Char → List Char
  | t :: rest, c => if t = c then rest else c :: t :: rest
  | [], c => [c]

/-- Stream all characters through the stack (top = most recent survivor). -/
def run (s : List Char) : List Char := s.foldl step []

/-- Reduced: no two adjacent characters are equal. -/
def Reduced : List Char → Prop
  | [] => True
  | [_] => True
  | x :: y :: rest => x ≠ y ∧ Reduced (y :: rest)

/-- SCHEME (fold): the de-duplication is a single streaming pass (a left fold). -/
theorem cls : IsFold (fun s : List Char => s.foldl step []) := ⟨step, [], fun _ => rfl⟩

theorem reduced_tail {x : Char} {rest : List Char} (h : Reduced (x :: rest)) : Reduced rest := by
  match rest with
  | [] => exact trivial
  | _ :: _ => exact h.2

/-- One step preserves reducedness. -/
theorem step_reduced : ∀ (st : List Char) (c : Char), Reduced st → Reduced (step st c) := by
  intro st
  induction st with
  | nil => intro c _; exact ⟨⟩
  | cons t rest ih =>
    intro c h
    simp only [step]
    by_cases htc : t = c
    · simp only [if_pos htc]; exact reduced_tail h
    · simp only [if_neg htc]
      refine ⟨fun hcontra => htc ?_, h⟩
      exact hcontra.symm

/-- NON-VACUITY: the survivor string is reduced — no adjacent duplicates remain. -/
theorem corr (s : List Char) : Reduced (run s) := by
  unfold run
  have key : ∀ (xs : List Char) st, Reduced st → Reduced (xs.foldl step st) := by
    intro xs
    induction xs with
    | nil => intro st h; exact h
    | cons c rest ih => intro st h; exact ih _ (step_reduced st c h)
  exact key s [] trivial

end LC.P1047
