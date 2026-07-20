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


/-- A concrete 2×2 grid `[[1,2],[4,3]]` (row r, column c ↦ value); cells off the grid read 0. -/
def exVal : Cell → ℤ := fun c =>
  if c = (0, 0) then 1 else if c = (0, 1) then 2
  else if c = (1, 1) then 3 else if c = (1, 0) then 4 else 0

/-- TEST VECTOR: from the 1-cell the increasing path 1→2→3→4 reaches the 4-cell; the off-grid
    cell (5,5) (value 0) is unreachable, since every step strictly increases the value above 1. -/
theorem vec : ((1, 0) : Cell) ∈ sol exVal (0, 0) ∧ ((5, 5) : Cell) ∉ sol exVal (0, 0) := by
  have h := corr exVal (0, 0)
  unfold spec at h
  rw [h]
  simp only [Set.mem_setOf_eq]
  constructor
  · exact .head (show incStep exVal (0, 0) (0, 1) by exact ⟨by simp [adjacent], by decide⟩)
      (.head (show incStep exVal (0, 1) (1, 1) by exact ⟨by simp [adjacent], by decide⟩)
        (.head (show incStep exVal (1, 1) (1, 0) by exact ⟨by simp [adjacent], by decide⟩) .refl))
  · intro hr
    have grow : ∀ v, Relation.ReflTransGen (incStep exVal) (0, 0) v →
        v = ((0, 0) : Cell) ∨ exVal (0, 0) < exVal v := by
      intro v hv
      induction hv with
      | refl => exact Or.inl rfl
      | tail _ step ih =>
        rename_i b c
        rcases ih with hb | hb
        · subst hb
          exact Or.inr step.2
        · exact Or.inr (lt_trans hb step.2)
    rcases grow (5, 5) hr with h5 | h5
    · exact absurd h5 (by decide)
    · rw [show exVal (5, 5) = 0 by decide, show exVal (0, 0) = 1 by decide] at h5
      omega

end LC.P0329
