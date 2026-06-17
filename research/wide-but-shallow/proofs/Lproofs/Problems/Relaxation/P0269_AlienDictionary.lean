import Lproofs.Generators

/-! @lc 269 | name:Alien Dictionary | scheme:relaxation | family:topo-sort | complexity:O(V+E) |
    source:https://leetcode.com/problems/alien-dictionary/

    Adjacent words yield ordering constraints `r u v` ("letter `u` must precede `v`"); a valid
    alphabet is a topological order of the constraint graph. CLASSIFICATION: as in Course Schedule,
    the set of letters that are *placeable* (every successor already placed) is the least fixpoint of
    one relaxation round — a valid order exists iff that set is everything (no cycle). `cls` certifies
    the relaxation fixpoint; `corr` ties it to the LEAST such set (no letter placed without a grounding
    successor chain). DROP the concrete ordering output. -/

namespace LC.P0269
open Interview

/-- One relaxation round: letter `u` is placeable once every letter it must precede (`r u v`) is. -/
def placeOp {V : Type*} (r : V → V → Prop) : Set V →o Set V where
  toFun S := {u | ∀ v, r u v → v ∈ S}
  monotone' _ _ hAB := fun _ hu v hrv => hAB (hu v hrv)

/-- Editorial Kahn topological order, modeled as the least-fixpoint placeable set. -/
def sol {V : Type*} (r : V → V → Prop) : Set V := OrderHom.lfp (placeOp r)

/-- Spec: the least relaxation-stable placeable set — no letter placed without a grounding chain. -/
def spec {V : Type*} (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (placeOp r)) S

/-- SCHEME (relaxation): the placeable set is a fixpoint of the relaxation operator. -/
theorem cls {V : Type*} (r : V → V → Prop) : placeOp r (sol r) = sol r :=
  bellman_fixpoint (placeOp r)

/-- CORRECT: it is the LEAST fixpoint — a cycle leaves some letter unplaceable, so no valid order. -/
theorem corr {V : Type*} (r : V → V → Prop) : spec r (sol r) :=
  bellman_isLeast (placeOp r)

end LC.P0269
