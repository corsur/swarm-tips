import Lproofs.Generators

/-! @lc 1368 | name:Minimum Cost to Make at Least One Valid Path in a Grid | scheme:relaxation |
    family:dijkstra | complexity:O(mn) | source:https://leetcode.com/problems/minimum-cost-to-make-at-least-one-valid-path-in-a-grid/

    0-1 BFS / Dijkstra relaxes the cost to redirect arrows along a path. CLASSIFICATION: the cells
    reachable from the start form the least fixpoint of one-step relaxation under the move relation `r`
    — the iterative cost-relaxation the 0-1 BFS performs. `cls` certifies the relaxation fixpoint;
    `corr` ties it to the LEAST such set. DROP the minimum-cost value. -/

namespace LC.P1368
open Interview

/-- Cells reachable from `start` under the (possibly-redirected) move relation, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: the least relaxation-stable set — the minimal reachable set, no over-relaxation. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {start} r)) S

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) : reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: it is the LEAST fixpoint — the minimal cost-relaxed reachable set from the start. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) :=
  bellman_isLeast (reachOp {start} r)

end LC.P1368
