import Lproofs.Generators

/-! @lc 329 | name:Longest Increasing Path in a Matrix | scheme:relaxation | family:dag-dp |
    complexity:O(mn) | source:https://leetcode.com/problems/longest-increasing-path-in-a-matrix/

    Longest strictly-increasing path in a grid; memoized DFS over the strictly-increasing DAG.
    CLASSIFICATION: the cells reachable from a start cell along strictly-increasing steps form the least
    fixpoint of one-step relaxation under the increasing-edge relation `r` — the DAG the DP traverses.
    `cls` certifies the relaxation fixpoint; `corr` ties it to genuine reachability along increasing
    steps. DROP the longest-path length value (the DP optimum). -/

namespace LC.P0329
open Interview

/-- Cells reachable from `start` along strictly-increasing steps, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the cells reachable from `start` along increasing steps. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the cells reachable along increasing steps from `start`. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0329
