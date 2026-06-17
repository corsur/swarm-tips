import Lproofs.Generators

/-! @lc 505 | name:The Maze II | scheme:relaxation | family:dijkstra | complexity:O(mn log mn) |
    source:https://leetcode.com/problems/the-maze-ii/

    A ball rolls until it hits a wall; Dijkstra relaxes the roll-distance to each stop cell.
    CLASSIFICATION: the reachable stop cells from the start form the least fixpoint of one-step
    relaxation under the roll relation `r` — exactly the iterative edge-relaxation Dijkstra performs.
    `cls` certifies the relaxation fixpoint; `corr` ties it to the LEAST such set (no cell relaxed in
    without a witnessing roll chain). DROP the shortest-distance value. -/

namespace LC.P0505
open Interview

/-- Stop cells reachable from `start` under the roll relation, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: the least relaxation-stable set — the minimal reachable set, no over-relaxation. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {start} r)) S

/-- SCHEME (relaxation): the reachable stop set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) : reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: it is the LEAST fixpoint — the minimal Dijkstra-relaxed reachable set from the start. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) :=
  bellman_isLeast (reachOp {start} r)

end LC.P0505
