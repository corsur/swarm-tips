import Lproofs.Generators

/-! @lc 433 | name:Minimum Genetic Mutation | scheme:relaxation | family:bfs | complexity:O(N·L) |
    source:https://leetcode.com/problems/minimum-genetic-mutation/

    BFS over gene strings: an edge joins two valid genes differing by one character. CLASSIFICATION:
    the set of genes reachable from the start gene under the one-mutation relation `r` (restricted to
    the valid bank) is the least fixpoint of one-step relaxation — the BFS frontier. `cls` certifies
    the relaxation fixpoint; `corr` ties it to genuine reachability (the end gene is mutable-to iff it
    lies in this set). DROP the minimum mutation count. -/

namespace LC.P0433
open Interview

/-- The genes reachable from `start` under one-mutation steps, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the genes reachable from `start` along one-mutation steps. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the reachable gene set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the genes reachable from `start`. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0433
