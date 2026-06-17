import Lproofs.Generators

/-! @lc 785 | name:Is Graph Bipartite? | scheme:relaxation | family:bfs-coloring | complexity:O(V+E) |
    source:https://leetcode.com/problems/is-graph-bipartite/

    BFS/DFS two-colors each connected component, declaring failure on a conflict. CLASSIFICATION: the
    traversal explores exactly the set of vertices reachable from a start vertex under the edge relation
    `r` — the least fixpoint of one-step relaxation (the BFS frontier the coloring rides on). `cls`
    certifies the relaxation fixpoint; `corr` ties it to genuine reachability. DROP the bipartite
    yes/no decision (the parity check layered on the traversal). -/

namespace LC.P0785
open Interview

/-- The component explored from `start`: the reachable set under `r`, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the vertices reachable from `start` along edges. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the explored component is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the reachable component the coloring traverses. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0785
