import Lproofs.Generators

/-! @lc 210 | name:Course Schedule II | scheme:relaxation | family:topo-sort | complexity:O(V+E) |
    source:https://leetcode.com/problems/course-schedule-ii/

    Return a valid course order (a topological order) given prerequisite edges. CLASSIFICATION
    (relaxation): the transitive prerequisites of a course --- everything that must be taken before it
    --- are the reachable set from that course under the CONCRETE prerequisite adjacency `g : ℕ → List ℕ`
    (the input format), as the least fixpoint of one-step relaxation. CORRECTNESS: the relaxation
    fixpoint is exactly that transitive-prerequisite set; a valid topological order exists iff no course
    reaches itself (no cycle). -/

namespace LC.P0210
open Interview

/-- Concrete prerequisite edge: course `u` directly requires course `v` (`v ∈ g u`). -/
def prereq (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- Transitive prerequisites of `start`: the courses reachable from it under `prereq`, as the least
    fixpoint of one-step relaxation. -/
def sol (g : ℕ → List ℕ) (start : ℕ) : Set ℕ := OrderHom.lfp (reachOp {start} (prereq g))

/-- Spec: exactly the courses reachable from `start` along a prerequisite chain. -/
def spec (g : ℕ → List ℕ) (start : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (prereq g) start v}

/-- SCHEME (relaxation): the prerequisite set is a fixpoint of one-step relaxation of `prereq`. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    reachOp {start} (prereq g) (sol g start) = sol g start :=
  reach_is_dp_fixpoint {start} (prereq g)

/-- CORRECT: the relaxation lfp is exactly the transitive-prerequisite set of `start` over the concrete
    graph --- the dependency closure a valid order must respect. -/
theorem corr (g : ℕ → List ℕ) (start : ℕ) : spec g start (sol g start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0210
