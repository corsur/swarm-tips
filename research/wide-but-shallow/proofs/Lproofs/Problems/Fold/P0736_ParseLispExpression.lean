import Lproofs.Schemes.Fold

/-! @lc 736 | name:Parse Lisp Expression | scheme:dp | family:string-other | complexity:O(n) |
    (editorial-relabelled fold->dp 2026-06-22: the accepted solution is recursive evaluation over the
    parse tree---a catamorphism; file kept under Fold/ for git history.)
    source:https://leetcode.com/problems/parse-lisp-expression/

    Evaluate a Lisp-like expression with integer literals, variables, `add`, `mult`, and `let`
    (scoped bindings). The accepted solution parses to an expression tree and evaluates it recursively
    in an environment. CLASSIFICATION (fold): evaluation is a catamorphism over the expression tree.
    CORRECTNESS (the scoping semantics, not full parser correctness): we certify the defining property
    of `let` --- a binding is read back by its variable: `eval (let x := e in x) = eval e` --- the
    scoping rule the whole evaluator rests on. -/

namespace LC.P0736

/-- A Lisp expression. -/
inductive Expr where
  | lit (n : ℤ)
  | var (x : ℕ)
  | add (a b : Expr)
  | mul (a b : Expr)
  | clet (x : ℕ) (e body : Expr)

/-- Evaluate in an environment (variable → value). -/
def eval (env : ℕ → ℤ) : Expr → ℤ
  | .lit n => n
  | .var x => env x
  | .add a b => eval env a + eval env b
  | .mul a b => eval env a * eval env b
  | .clet x e body => eval (Function.update env x (eval env e)) body

/-- SCHEME (fold / catamorphism): evaluation folds the expression tree --- a compound node is its
    operator applied to the evaluated subexpressions. -/
theorem cls (env : ℕ → ℤ) (a b : Expr) : eval env (.add a b) = eval env a + eval env b := rfl

/-- CORRECT: a `let` binding is read back by its bound variable --- `eval (let x := e in x) = eval e`.
    The scoping rule the evaluator relies on (proven via the environment update, not by definition). -/
theorem corr (env : ℕ → ℤ) (x : ℕ) (e : Expr) :
    eval env (.clet x e (.var x)) = eval env e := by
  simp [eval]

end LC.P0736
