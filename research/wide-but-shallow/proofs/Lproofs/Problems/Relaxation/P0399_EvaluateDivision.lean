import Lproofs.Generators

/-! @lc 399 | name:Evaluate Division | scheme:relaxation | family:union-find | complexity:O(Q·(V+E)) |
    source:https://leetcode.com/problems/evaluate-division/

    Variables linked by known ratios form a graph; a query `a / b` is answerable iff `b` is reachable
    from `a`. CLASSIFICATION: the set of variables reachable from `a` under the ratio-edge relation `r`
    is the least fixpoint of one-step relaxation (the union-find / DFS the queries ride on). `cls`
    certifies the relaxation fixpoint; `corr` ties it to the LEAST reachable set (no variable joined
    without a witnessing ratio chain). DROP the computed quotient value. -/

namespace LC.P0399
open Interview

/-- Variables reachable from `a` under the ratio relation, as a relaxation lfp. -/
def sol {V : Type*} (a : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {a} r)

/-- Spec: the least relaxation-stable set — the minimal connected component, no over-relaxation. -/
def spec {V : Type*} (a : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {a} r)) S

/-- SCHEME (relaxation): the reachable variable set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (a : V) (r : V → V → Prop) : reachOp {a} r (sol a r) = sol a r :=
  reach_is_dp_fixpoint {a} r

/-- CORRECT: it is the LEAST fixpoint — the minimal component of variables linked to `a`. -/
theorem corr {V : Type*} (a : V) (r : V → V → Prop) : spec a r (sol a r) :=
  bellman_isLeast (reachOp {a} r)

end LC.P0399
