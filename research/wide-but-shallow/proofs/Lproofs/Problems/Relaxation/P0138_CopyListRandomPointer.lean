import Lproofs.Generators

/-! @lc 138 | name:Copy List with Random Pointer | scheme:relaxation | family:graph-traversal |
    complexity:O(n) | source:https://leetcode.com/problems/copy-list-with-random-pointer/

    Deep-copy a linked list whose nodes also carry arbitrary `random` pointers; a traversal clones every
    node reachable via `next`/`random`. CLASSIFICATION: the set of nodes the copy visits is exactly the
    set reachable from the head under the pointer relation `r` (`next` or `random`) — the least fixpoint
    of one-step relaxation. `cls` certifies the relaxation fixpoint; `corr` ties it to genuine
    reachability (every reachable node is cloned, none beyond). DROP the pointer-rewiring construction. -/

namespace LC.P0138
open Interview

/-- The cloned nodes: the reachable set from `head` under `next`/`random`, as a relaxation lfp. -/
def sol {V : Type*} (head : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {head} r)

/-- Spec: exactly the nodes reachable from `head` along the pointer relation. -/
def spec {V : Type*} (head : V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | Relation.ReflTransGen r head v}

/-- SCHEME (relaxation): the cloned set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (head : V) (r : V → V → Prop) :
    reachOp {head} r (sol head r) = sol head r :=
  reach_is_dp_fixpoint {head} r

/-- CORRECT: the relaxation lfp is exactly the reachable nodes the copy traversal visits. -/
theorem corr {V : Type*} (head : V) (r : V → V → Prop) : spec head r (sol head r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0138
