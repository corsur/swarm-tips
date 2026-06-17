import Lproofs.Generators

/-! @lc 3387 | name:Graph Reachability | scheme:relaxation | family:graph-traversal | complexity:O(V+E) |
    source:https://leetcode.com/

    CLASSIFICATION: the nodes reachable from the start under the graph relation is the least fixpoint of one-step relaxation under the relation `r` — the
    iterative graph traversal the accepted algorithm performs. `cls` certifies the relaxation fixpoint;
    `corr` ties it to the LEAST such set (nothing relaxed in without a witnessing chain). DROP any
    distance/count value. -/

namespace LC.P3387
open Interview

def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {start} r)) S

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) : reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: it is the LEAST fixpoint — the minimal reachable set from the start. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) :=
  bellman_isLeast (reachOp {start} r)

end LC.P3387
