import Lproofs.Generators

/-! @lc 1584 | name:Min Cost to Connect All Points | scheme:relaxation | family:union-find |
    complexity:O(n² log n) | source:https://leetcode.com/problems/min-cost-to-connect-all-points/

    A minimum spanning tree (Prim/Kruskal with union-find) connects all points. CLASSIFICATION: the set
    of points connected to a start point is the least fixpoint of one-step relaxation under the
    chosen-edge relation `r` — the union-find growth the MST performs. `cls` certifies the relaxation
    fixpoint; `corr` ties it to the LEAST connected set (no point joined without a witnessing edge
    chain). DROP the total-cost value. -/

namespace LC.P1584
open Interview

/-- Points connected to `start` under the spanning-edge relation, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: the least relaxation-stable set — the minimal connected component, no over-relaxation. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {start} r)) S

/-- SCHEME (relaxation): the connected set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) : reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: it is the LEAST fixpoint — the minimal spanning-connected component containing `start`. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) :=
  bellman_isLeast (reachOp {start} r)

end LC.P1584
