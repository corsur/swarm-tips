import Lproofs.Generators

/-! @lc 1091 | name:Shortest Path in Binary Matrix | scheme:relaxation | family:bfs | complexity:O(n²) |
    source:https://leetcode.com/problems/shortest-path-in-binary-matrix/

    BFS the shortest 8-directional clear path from top-left to bottom-right. CLASSIFICATION: the set of
    cells reachable from the start under the clear-adjacency relation `r` is the least fixpoint of
    one-step relaxation — the BFS frontier. `cls` certifies the relaxation fixpoint; `corr` ties it to
    genuine reachability (the goal is reachable iff a clear path exists). DROP the shortest-length
    count. -/

namespace LC.P1091
open Interview

/-- Cells reachable from `start` under clear 8-adjacency, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the cells reachable from `start` along clear adjacency. -/
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

end LC.P1091
