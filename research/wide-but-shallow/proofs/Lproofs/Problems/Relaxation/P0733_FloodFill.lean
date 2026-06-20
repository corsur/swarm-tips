import Lproofs.Generators

/-! @lc 733 | name:Flood Fill | scheme:relaxation | family:dfs-flood | complexity:O(n) |
    source:https://leetcode.com/problems/flood-fill/

    Flood fill recolours every cell reachable from the start cell by a path of orthogonally-adjacent
    cells that all share the start cell's ORIGINAL colour. CLASSIFICATION: that recoloured region is
    exactly the reachable set from `{start}` under the concrete flood relation `floodRel` (a step is an
    orthogonal neighbour of the same original colour) — the least fixpoint of one-step relaxation.
    `cls` certifies the relaxation fixpoint; `corr` proves the fixpoint is exactly the flood region of
    the concrete grid (not an abstract placeholder relation). -/

namespace LC.P0733
open Interview

/-- A grid cell. -/
abbrev Cell := ℤ × ℤ

/-- Orthogonal (4-direction) adjacency on the integer grid. -/
def adjacent (p q : Cell) : Prop :=
  (p.1 = q.1 ∧ (q.2 = p.2 + 1 ∨ p.2 = q.2 + 1)) ∨ (p.2 = q.2 ∧ (q.1 = p.1 + 1 ∨ p.1 = q.1 + 1))

/-- The flood step relation for a concrete `grid` and `start` cell: an orthogonal neighbour that
    carries the start cell's original colour. This is the actual flood-fill rule, not a free `r`. -/
def floodRel (grid : Cell → ℤ) (start : Cell) (p q : Cell) : Prop :=
  adjacent p q ∧ grid q = grid start

/-- The recoloured region: the cells reachable from `start` under `floodRel`, as the least fixpoint
    of one-step relaxation. -/
def sol (grid : Cell → ℤ) (start : Cell) : Set Cell :=
  OrderHom.lfp (reachOp {start} (floodRel grid start))

/-- Spec: the region is exactly the cells reachable from `start` along same-colour orthogonal steps. -/
def spec (grid : Cell → ℤ) (start : Cell) (S : Set Cell) : Prop :=
  S = {v | Relation.ReflTransGen (floodRel grid start) start v}

/-- SCHEME (relaxation): the region is a fixpoint of one-step relaxation of the concrete flood relation. -/
theorem cls (grid : Cell → ℤ) (start : Cell) :
    reachOp {start} (floodRel grid start) (sol grid start) = sol grid start :=
  reach_is_dp_fixpoint {start} (floodRel grid start)

/-- CORRECT: the relaxation lfp is exactly the flood region of this grid — the cells reachable from
    `start` by orthogonal steps through cells of the start's original colour. -/
theorem corr (grid : Cell → ℤ) (start : Cell) : spec grid start (sol grid start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0733
