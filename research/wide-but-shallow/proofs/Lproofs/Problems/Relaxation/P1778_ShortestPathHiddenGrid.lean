import Lproofs.Generators

/-! @lc 1778 | name:Shortest Path in a Hidden Grid | scheme:relaxation | family:bfs | complexity:O(mn) |
    source:https://leetcode.com/problems/shortest-path-in-a-hidden-grid/

    Explore the grid, then BFS the shortest path to the target. CLASSIFICATION: the set of cells
    reachable from the start under the open-move relation `r` is the least fixpoint of one-step
    relaxation — the BFS frontier. `cls` certifies the relaxation fixpoint; `corr` ties it to genuine
    reachability (the target is reachable iff a path exists). DROP the shortest-length count. -/

namespace LC.P1778
open Interview

/-- Cells reachable from `start` under open moves, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the cells reachable from `start` along open moves. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the cells reachable from the start. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P1778
