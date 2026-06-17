import Lproofs.Generators

/-! @lc 1202 | name:Smallest String With Swaps | scheme:relaxation | family:union-find |
    complexity:O(n log n) | source:https://leetcode.com/problems/smallest-string-with-swaps/ -/

namespace LC.P1202
open Interview

/-- `r u v`: positions `u`, `v` are directly swappable. The connected component of a position set
    `s` (within which characters may be freely rearranged) is the reachable set under `r` — the
    least fixpoint of one-step relaxation (union-find). -/
def sol {V : Type*} (s : Set V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp s r)

/-- Spec: the component is exactly the swap-reachable set — the genuine connected component within
    which characters may be rearranged (Mathlib's reflexive-transitive closure of `r` from `s`). -/
def spec {V : Type*} (s : Set V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | ∃ u ∈ s, Relation.ReflTransGen r u v}

/-- SCHEME (relaxation): the connected component is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (s : Set V) (r : V → V → Prop) : reachOp s r (sol s r) = sol s r :=
  reach_is_dp_fixpoint s r

/-- CORRECT: the relaxation lfp is exactly the swap-reachable connected component — so `sol`
    computes the real component, not just an abstract fixpoint. -/
theorem corr {V : Type*} (s : Set V) (r : V → V → Prop) : spec s r (sol s r) :=
  lfp_reachOp_eq_reachable s r

end LC.P1202
