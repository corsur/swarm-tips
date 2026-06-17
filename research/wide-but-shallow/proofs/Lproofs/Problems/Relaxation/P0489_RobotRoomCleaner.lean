import Lproofs.Generators

/-! @lc 489 | name:Robot Room Cleaner | scheme:relaxation | family:dfs-flood | complexity:O(cells) |
    source:https://leetcode.com/problems/robot-room-cleaner/

    A robot DFS-cleans every reachable cell, backtracking at walls. CLASSIFICATION: the set of cells the
    robot visits is exactly the set reachable from its start cell under the open-move relation `r` —
    the least fixpoint of one-step relaxation (the DFS frontier). `cls` certifies the relaxation
    fixpoint; `corr` ties it to genuine reachability (every reachable cell is cleaned, none beyond).
    DROP the movement/backtracking choreography. -/

namespace LC.P0489
open Interview

/-- Cells the robot visits: the reachable set from `start` under open moves, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the cells reachable from `start` along open moves. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the cleaned set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the cells reachable from the robot's start. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0489
