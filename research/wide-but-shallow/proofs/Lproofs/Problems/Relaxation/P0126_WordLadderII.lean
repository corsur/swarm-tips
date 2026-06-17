import Lproofs.Generators

/-! @lc 126 | name:Word Ladder II | scheme:relaxation | family:bfs | complexity:O(N·L²) |
    source:https://leetcode.com/problems/word-ladder-ii/

    BFS layered shortest transformation sequences between words differing by one letter. CLASSIFICATION:
    the set of words reachable from the start word under the one-letter-difference relation `r` is the
    least fixpoint of one-step relaxation (the BFS layers the path reconstruction rides on). `cls`
    certifies the relaxation fixpoint; `corr` ties it to genuine reachability. DROP the shortest-length
    and path-enumeration steps. -/

namespace LC.P0126
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

end LC.P0126
