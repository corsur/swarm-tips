import Lproofs.Generators

/-! @lc 261 | name:Graph Valid Tree | scheme:relaxation | family:union-find | complexity:O(V+E) |
    source:https://leetcode.com/problems/graph-valid-tree/

    A graph is a tree iff it is connected and acyclic; union-find / BFS checks connectivity (and a
    union of already-joined endpoints flags a cycle). CLASSIFICATION: the set reachable from any vertex
    under the (symmetric) edge relation `r` is the least fixpoint of one-step relaxation — the graph is
    connected iff that set is everything. `cls` certifies the relaxation fixpoint; `corr` ties it to the
    LEAST reachable set (no vertex joined without a witnessing edge chain). DROP the tree yes/no. -/

namespace LC.P0261
open Interview

/-- The reachable set from `root` under `r`, as a relaxation lfp (the union-find component). -/
def sol {V : Type*} (root : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {root} r)

/-- Spec: the least relaxation-stable set — the minimal component, no over-relaxation. -/
def spec {V : Type*} (root : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {root} r)) S

/-- SCHEME (relaxation): the reachable component is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (root : V) (r : V → V → Prop) :
    reachOp {root} r (sol root r) = sol root r :=
  reach_is_dp_fixpoint {root} r

/-- CORRECT: it is the LEAST fixpoint — the minimal component (connected iff it is all vertices). -/
theorem corr {V : Type*} (root : V) (r : V → V → Prop) : spec root r (sol root r) :=
  bellman_isLeast (reachOp {root} r)

end LC.P0261
