import Lproofs.Generators

/-! @lc 733 | name:Flood Fill | scheme:relaxation | family:dfs-flood | complexity:O(n) |
    source:https://leetcode.com/problems/flood-fill/

    Flood fill recolors every cell reachable from the start cell by a path of same-original-color
    neighbours. CLASSIFICATION: that recolored region is exactly the reachable set from `{start}`
    under the flood relation `r` (`q` is a same-color neighbour of `p`) — the least fixpoint of
    one-step relaxation. `cls` certifies it is a relaxation fixpoint; `corr` ties the fixpoint to the
    genuine reachable region (not an abstract placeholder). -/

namespace LC.P0733
open Interview

/-- The recolored region: the reachable set from the start cell under the same-color adjacency `r`,
    as the least fixpoint of one-step relaxation. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: the region is exactly the cells reachable from `start` along `r`. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the region is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the reachable flood region. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0733
