import Lproofs.Generators

/-! @lc 323 | name:Number of Connected Components in an Undirected Graph | scheme:relaxation |
    family:union-find | complexity:O(V+E) |
    source:https://leetcode.com/problems/number-of-connected-components-in-an-undirected-graph/

    Count connected components. CLASSIFICATION: the component containing a vertex `v` is exactly the
    set reachable from `{v}` under the (symmetric) edge relation `r` — the least fixpoint of one-step
    relaxation (union-find / BFS). `cls` certifies the relaxation fixpoint; `corr` ties it to the LEAST
    such set (no vertex joined without a witnessing edge chain). DROP the component count. -/

namespace LC.P0323
open Interview

/-- The component of `v`: the reachable set from `{v}` under `r`, as a relaxation lfp. -/
def sol {V : Type*} (v : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {v} r)

/-- Spec: the least relaxation-stable set — the minimal component, no over-relaxation. -/
def spec {V : Type*} (v : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {v} r)) S

/-- SCHEME (relaxation): the component is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (v : V) (r : V → V → Prop) : reachOp {v} r (sol v r) = sol v r :=
  reach_is_dp_fixpoint {v} r

/-- CORRECT: it is the LEAST fixpoint — the minimal connected component containing `v`. -/
theorem corr {V : Type*} (v : V) (r : V → V → Prop) : spec v r (sol v r) :=
  bellman_isLeast (reachOp {v} r)

end LC.P0323
