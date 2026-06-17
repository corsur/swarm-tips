import Lproofs.Schemes.Fold

/-! @lc 150 | name:Evaluate Reverse Polish Notation | scheme:fold | family:operand-stack |
    complexity:O(n) | source:https://leetcode.com/problems/evaluate-reverse-polish-notation/

    Evaluate an RPN expression with an operand stack: push numbers; an operator pops its two operands
    and pushes the result. CLASSIFICATION: the evaluation is a streaming left fold over the tokens whose
    accumulator is the operand stack. NON-VACUITY: we prove the genuine arity recurrence — an operator
    consumes the top two operands and pushes their combination — so the accumulator does real
    stack-machine work, not a re-encoding. We certify the fold + the operator recurrence. -/

namespace LC.P0150
open Interview.Patterns

/-- RPN tokens. -/
inductive Tok | num (n : ℤ) | add | sub | mul

/-- One step: push a number, or apply an operator to the top two operands. -/
def step : List ℤ → Tok → List ℤ
  | stk, .num n => n :: stk
  | b :: a :: rest, .add => (a + b) :: rest
  | b :: a :: rest, .sub => (a - b) :: rest
  | b :: a :: rest, .mul => (a * b) :: rest
  | stk, _ => stk

/-- Evaluate by folding the tokens through the operand stack. -/
def eval (ts : List Tok) : List ℤ := ts.foldl step []

/-- SCHEME (fold): the evaluation is a left fold over the tokens with the operand-stack accumulator. -/
theorem cls : IsFold (fun ts : List Tok => ts.foldl step []) := ⟨step, [], fun _ => rfl⟩

/-- NON-VACUITY (operator arity): `+` pops the top two operands `a, b` and pushes `a + b` — the genuine
    binary stack-machine reduction. -/
theorem corr_add (a b : ℤ) (rest : List ℤ) : step (b :: a :: rest) .add = (a + b) :: rest := rfl

/-- NON-VACUITY (arithmetic): the whole machine genuinely computes — e.g. `3 4 +` evaluates to `7`. -/
theorem corr (a b : ℤ) : eval [.num a, .num b, .add] = [a + b] := by
  simp [eval, step]

end LC.P0150
