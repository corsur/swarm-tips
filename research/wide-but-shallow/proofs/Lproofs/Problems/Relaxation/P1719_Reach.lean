import Lproofs.Generators

/-! @lc 1719 | name:Number Of Ways To Reconstruct A Tree | scheme:relaxation | family:graph-other |
    complexity:O(V+E) | source:https://leetcode.com/problems/number-of-ways-to-reconstruct-a-tree/

    Reconstruction reasons over ancestor/descendant reachability in the pair graph. CLASSIFICATION: the
    set of nodes reachable from a node under the ancestor relation `r` is the least fixpoint of one-step
    relaxation — the graph traversal the check performs. `cls` certifies the relaxation fixpoint; `corr`
    ties it to the LEAST reachable set (nothing relaxed in without a witnessing chain). DROP the
    counting/uniqueness verdict. -/

namespace LC.P1719
open Interview

def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {start} r)) S

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) : reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: it is the LEAST fixpoint — the minimal ancestor-reachable set from the node. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) :=
  bellman_isLeast (reachOp {start} r)

end LC.P1719
