import Lproofs.Generators

/-! @lc 127 | name:Word Ladder | scheme:relaxation | family:bfs | complexity:O(N·L²) |
    source:https://leetcode.com/problems/word-ladder/

    BFS for the shortest transformation chain between two words differing by one letter at a time.
    CLASSIFICATION: the set of words reachable from the start word under the one-letter-difference
    relation `r` is the least fixpoint of one-step relaxation — the BFS frontier. `cls` certifies the
    relaxation fixpoint; `corr` ties it to genuine reachability (the end word is reachable iff a ladder
    exists). DROP the shortest-length count. -/

namespace LC.P0127
open Interview

/-- The words reachable from `start` under one-letter steps, as a relaxation lfp. -/
def sol {V : Type*} (start : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the words reachable from `start` along one-letter steps. -/
def spec {V : Type*} (start : V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the reachable word set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (start : V) (r : V → V → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the words reachable from `start`. -/
theorem corr {V : Type*} (start : V) (r : V → V → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0127
