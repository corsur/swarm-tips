import Lproofs.Generators

/-! @lc 207 | name:Course Schedule | scheme:relaxation | family:topo-sort | complexity:O(V+E) |
    source:https://leetcode.com/problems/course-schedule/ -/

namespace LC.P0207
open Interview

/-- One relaxation round of "completability": a course `u` is completable once all its
    prerequisites (`r u v`) are. Monotone, so its least fixpoint is the completable set. -/
def compOp {V : Type*} (r : V → V → Prop) : Set V →o Set V where
  toFun S := {u | ∀ v, r u v → v ∈ S}
  monotone' _ _ hAB := fun _ hu v hrv => hAB (hu v hrv)

/-- Editorial topological-sort / Kahn solution, modeled as the least-fixpoint completable set
    (the DP/relaxation characterization). "Can finish all" iff this set is everything. -/
def sol {V : Type*} (r : V → V → Prop) : Set V := OrderHom.lfp (compOp r)

/-- Spec: the answer is the least relaxation-stable completable set (Bellman-optimal — a course
    is included only with a finite witnessing prerequisite chain, i.e. no cycle blocks it). -/
def spec {V : Type*} (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (compOp r)) S

/-- SCHEME (relaxation): the completable set is a fixpoint of the relaxation operator. -/
theorem cls {V : Type*} (r : V → V → Prop) : compOp r (sol r) = sol r :=
  bellman_fixpoint (compOp r)

/-- CORRECT: it is the LEAST fixpoint — the minimal completable set, no course included without a
    grounding prerequisite chain. -/
theorem corr {V : Type*} (r : V → V → Prop) : spec r (sol r) :=
  bellman_isLeast (compOp r)

end LC.P0207
