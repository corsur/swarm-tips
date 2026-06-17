import Lproofs.Generators

/-! @lc 1462 | name:Course Schedule IV | scheme:relaxation | family:topo-sort | complexity:O(V*E) |
    source:https://leetcode.com/problems/course-schedule-iv/ -/

namespace LC.P1462
open Interview

/-- `r u v`: `v` is a direct prerequisite of `u`. The prerequisite-closure of a course set `s`
    (which answers the "is X a prerequisite of Y" queries) is the reachable set under `r` — the
    least fixpoint of one-step relaxation. -/
def sol {V : Type*} (s : Set V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp s r)

/-- Spec: the closure is exactly the prerequisite-reachable set — the genuine answer to the
    "is X a prerequisite of Y" queries (Mathlib's reflexive-transitive closure of `r` from `s`). -/
def spec {V : Type*} (s : Set V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | ∃ u ∈ s, Relation.ReflTransGen r u v}

/-- SCHEME (relaxation): the prerequisite-closure is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (s : Set V) (r : V → V → Prop) : reachOp s r (sol s r) = sol s r :=
  reach_is_dp_fixpoint s r

/-- CORRECT: the relaxation lfp is exactly the prerequisite-reachable set — so `sol` computes the
    real prerequisite relation, not just an abstract fixpoint. -/
theorem corr {V : Type*} (s : Set V) (r : V → V → Prop) : spec s r (sol s r) :=
  lfp_reachOp_eq_reachable s r

end LC.P1462
