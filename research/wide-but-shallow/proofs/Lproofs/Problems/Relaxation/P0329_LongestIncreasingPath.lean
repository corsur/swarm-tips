import Lproofs.Generators

/-! @lc 329 | name:Longest Increasing Path in a Matrix | scheme:relaxation | family:dfs-flood |
    complexity:O(V+E) | source:https://leetcode.com/problems/longest-increasing-path-in-a-matrix/

    The accepted solution is a DFS-with-memo that follows strictly-increasing steps to orthogonally
    adjacent cells. CLASSIFICATION (relaxation): the cells reachable from a start cell along strictly
    increasing orthogonal steps form the reachable set under the CONCRETE step relation `incStep`
    (orthogonal neighbour with strictly larger value on the concrete grid `val`), as a least fixpoint of
    one-step relaxation. CORRECTNESS: the relaxation fixpoint is exactly that increasing-reachable set
    over the concrete grid --- the search space the longest-path DFS explores. -/

namespace LC.P0329
open Interview

/-- A grid cell. -/
abbrev Cell := ℤ × ℤ

/-- Orthogonal (4-direction) adjacency on the integer grid. -/
def adjacent (p q : Cell) : Prop :=
  (p.1 = q.1 ∧ (q.2 = p.2 + 1 ∨ p.2 = q.2 + 1)) ∨ (p.2 = q.2 ∧ (q.1 = p.1 + 1 ∨ p.1 = q.1 + 1))

/-- The increasing-step relation on a concrete grid: an orthogonal neighbour with a strictly larger
    value. This is the actual movement rule, not a free relation. -/
def incStep (val : Cell → ℤ) (p q : Cell) : Prop := adjacent p q ∧ val p < val q

/-- Cells reachable from `start` along strictly-increasing orthogonal steps, as a least fixpoint. -/
def sol (val : Cell → ℤ) (start : Cell) : Set Cell :=
  OrderHom.lfp (reachOp {start} (incStep val))

/-- Spec: exactly the cells reachable from `start` along an increasing orthogonal path. -/
def spec (val : Cell → ℤ) (start : Cell) (S : Set Cell) : Prop :=
  S = {v | Relation.ReflTransGen (incStep val) start v}

/-- SCHEME (relaxation): the increasing-reachable set is a fixpoint of one-step relaxation of `incStep`. -/
theorem cls (val : Cell → ℤ) (start : Cell) :
    reachOp {start} (incStep val) (sol val start) = sol val start :=
  reach_is_dp_fixpoint {start} (incStep val)

/-- CORRECT: the relaxation lfp is exactly the increasing-reachable set from `start` over the concrete
    grid --- the cells an increasing path can visit. -/
theorem corr (val : Cell → ℤ) (start : Cell) : spec val start (sol val start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0329
