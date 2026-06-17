import Lproofs.Generators

/-! @lc 1971 | name:Find if Path Exists in Graph | scheme:relaxation | family:union-find |
    complexity:O(V+E) | source:https://leetcode.com/problems/find-if-path-exists-in-graph/ -/

namespace LC.P1971
open Interview

/-- Editorial union-find / BFS reachable set, modeled as the least fixpoint of one-step
    relaxation from the source set `s` along the (symmetric) edge relation `r`. -/
def sol {V : Type*} (s : Set V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp s r)

/-- Spec: the answer is the least relaxation-stable set — the minimal set containing the sources
    and closed under one edge step, i.e. exactly the reachable set with no over-relaxation. -/
def spec {V : Type*} (s : Set V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp s r)) S

/-- SCHEME (relaxation): the solution is a fixpoint of the relaxation operator — relaxation has
    converged (one more BFS/union round adds nothing). -/
theorem cls {V : Type*} (s : Set V) (r : V → V → Prop) :
    reachOp s r (sol s r) = sol s r :=
  reach_is_dp_fixpoint s r

/-- CORRECT: the solution is the LEAST fixpoint — the minimal reachable set, Bellman-optimal
    (no vertex is marked reachable without a witnessing path). -/
theorem corr {V : Type*} (s : Set V) (r : V → V → Prop) : spec s r (sol s r) :=
  bellman_isLeast (reachOp s r)

end LC.P1971
