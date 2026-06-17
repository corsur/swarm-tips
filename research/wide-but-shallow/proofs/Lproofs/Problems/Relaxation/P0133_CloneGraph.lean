import Lproofs.Generators

/-! @lc 133 | name:Clone Graph | scheme:relaxation | family:bfs | complexity:O(V+E) |
    source:https://leetcode.com/problems/clone-graph/

    Deep-copy a connected graph: BFS/DFS from the given node, cloning each newly-discovered vertex.
    CLASSIFICATION: the set of vertices the traversal visits (hence clones) is exactly the set reachable
    from the start node under the edge relation `r` — the least fixpoint of one-step relaxation. `cls`
    certifies the relaxation fixpoint; `corr` ties it to genuine reachability (every reachable node is
    cloned, none beyond). DROP the pointer-level copy construction. -/

namespace LC.P0133
open Interview

/-- The cloned vertices: the reachable set from `start` under `r`, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the vertices reachable from `start` along edges. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the cloned set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the reachable vertices the clone traversal visits. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0133
