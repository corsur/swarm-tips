import Lproofs.Generators

/-! @lc 210 | name:Course Schedule II | scheme:relaxation | family:topo-sort | complexity:O(V+E) |
    source:https://leetcode.com/problems/course-schedule-ii/

    Return a valid course order (a topological order) or empty if a cycle blocks it. CLASSIFICATION:
    as in Course Schedule, the set of *completable* courses (every prerequisite already completable) is
    the least fixpoint of one relaxation round; a full ordering exists iff that set is everything.
    `cls` certifies the relaxation fixpoint; `corr` ties it to the LEAST such set (no course included
    without a grounding prerequisite chain). DROP the concrete ordering output. -/

namespace LC.P0210
open Interview

/-- One relaxation round: course `u` is completable once every prerequisite (`r u v`) is. -/
def compOp {V : Type*} (r : V → V → Prop) : Set V →o Set V where
  toFun S := {u | ∀ v, r u v → v ∈ S}
  monotone' _ _ hAB := fun _ hu v hrv => hAB (hu v hrv)

/-- Editorial Kahn topological order, modeled as the least-fixpoint completable set. -/
def sol {V : Type*} (r : V → V → Prop) : Set V := OrderHom.lfp (compOp r)

/-- Spec: the least relaxation-stable completable set — no course placed without a grounding chain. -/
def spec {V : Type*} (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (compOp r)) S

/-- SCHEME (relaxation): the completable set is a fixpoint of the relaxation operator. -/
theorem cls {V : Type*} (r : V → V → Prop) : compOp r (sol r) = sol r :=
  bellman_fixpoint (compOp r)

/-- CORRECT: it is the LEAST fixpoint — a cycle leaves some course unplaceable, so no valid order. -/
theorem corr {V : Type*} (r : V → V → Prop) : spec r (sol r) :=
  bellman_isLeast (compOp r)

end LC.P0210
