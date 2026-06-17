import Lproofs.Generators

/-! @lc 934 | name:Shortest Bridge | scheme:relaxation | family:bfs | complexity:O(mn) |
    source:https://leetcode.com/problems/shortest-bridge/

    Flood one island, then BFS outward to the second. CLASSIFICATION: each phase explores exactly the
    set of cells reachable from its sources under the step relation `r` — the least fixpoint of one-step
    relaxation (the flood, then the expanding BFS frontier). `cls` certifies the relaxation fixpoint;
    `corr` ties it to genuine reachability from the source set. DROP the bridge-length count. -/

namespace LC.P0934
open Interview

/-- Cells reachable from the source set under `r`, as a relaxation lfp (multi-source). -/
def sol {V : Type*} (sources : Set V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp sources r)

/-- Spec: exactly the cells reachable from some source along `r`. -/
def spec {V : Type*} (sources : Set V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | ∃ u ∈ sources, Relation.ReflTransGen r u v}

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (sources : Set V) (r : V → V → Prop) :
    reachOp sources r (sol sources r) = sol sources r :=
  reach_is_dp_fixpoint sources r

/-- CORRECT: the relaxation lfp is exactly the cells reachable from the sources. -/
theorem corr {V : Type*} (sources : Set V) (r : V → V → Prop) : spec sources r (sol sources r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]

end LC.P0934
