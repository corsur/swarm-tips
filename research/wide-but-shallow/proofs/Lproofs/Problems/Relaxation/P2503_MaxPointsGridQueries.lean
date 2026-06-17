import Lproofs.Generators

/-! @lc 2503 | name:Maximum Number of Points From Grid Queries | scheme:relaxation | family:union-find |
    complexity:O(nm log) | source:https://leetcode.com/problems/maximum-number-of-points-from-grid-queries/ -/

namespace LC.P2503
open Interview

/-- `r u v`: cells `u`, `v` are connected at the current query threshold. The component reachable
    from the start cell `s` (whose size is the query's score) is the least fixpoint of one-step
    relaxation (incremental union-find as the threshold rises). -/
def sol {V : Type*} (s : Set V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp s r)

/-- Spec: the least relaxation-stable reachable component from the start. -/
def spec {V : Type*} (s : Set V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp s r)) S

/-- SCHEME (relaxation): the component is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (s : Set V) (r : V → V → Prop) : reachOp s r (sol s r) = sol s r :=
  reach_is_dp_fixpoint s r

/-- CORRECT: it is the LEAST fixpoint — the minimal reachable component. -/
theorem corr {V : Type*} (s : Set V) (r : V → V → Prop) : spec s r (sol s r) :=
  bellman_isLeast (reachOp s r)

end LC.P2503
