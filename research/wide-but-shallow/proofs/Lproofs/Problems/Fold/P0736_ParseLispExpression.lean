import Lproofs.Schemes.Fold

/-! @lc 736 | name:Parse Lisp Expression | scheme:dp | family:string-other | complexity:O(n) |
    (editorial-relabelled fold->dp 2026-06-22: the accepted solution is recursive evaluation over the
    parse tree---a catamorphism; file kept under Fold/ for git history.)
    source:https://leetcode.com/problems/parse-lisp-expression/

    Evaluate a Lisp-like expression with integer literals, variables, `add`, `mult`, and `let`
    (scoped bindings). The accepted solution parses to an expression tree and evaluates it recursively
    in an environment. CLASSIFICATION (fold): evaluation is a catamorphism over the expression tree.
    CORRECTNESS (the scoping semantics, not full parser correctness): we certify the defining property
    of `let` --- a binding is read back by its variable: `sol (let x := e in x) = sol e` --- the
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
def sol (env : ℕ → ℤ) : Expr → ℤ
  | .lit n => n
  | .var x => env x
  | .add a b => sol env a + sol env b
  | .mul a b => sol env a * sol env b
  | .clet x e body => sol (Function.update env x (sol env e)) body

/-- SCHEME (fold / catamorphism): evaluation folds the expression tree --- a compound node is its
    operator applied to the evaluated subexpressions. -/
theorem cls (env : ℕ → ℤ) (a b : Expr) : sol env (.add a b) = sol env a + sol env b := rfl

/-- CORRECT: a `let` binding is read back by its bound variable --- `sol (let x := e in x) = sol e`.
    The scoping rule the evaluator relies on (proven via the environment update, not by definition). -/
theorem corr (env : ℕ → ℤ) (x : ℕ) (e : Expr) :
    sol env (.clet x e (.var x)) = sol env e := by
  simp [sol]


/-- Official example 1: (let x 2 (mult x (let x 3 y 4 (add x y)))) with x, y as variables 0, 1. -/
def exExpr : Expr :=
  .clet 0 (.lit 2) (.mul (.var 0) (.clet 0 (.lit 3) (.clet 1 (.lit 4)
    (.add (.var 0) (.var 1)))))

/-- GROUND INSTANCE (official example 1): the expression evaluates to 14 (inner x = 3 shadows the
    outer x = 2 only inside its own let-body). -/
theorem vec : sol (fun _ => 0) exExpr = 14 := by decide

end LC.P0736
