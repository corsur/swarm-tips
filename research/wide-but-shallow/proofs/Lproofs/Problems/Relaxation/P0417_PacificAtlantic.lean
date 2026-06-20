import Lproofs.Generators

/-! @lc 417 | name:Pacific Atlantic Water Flow | scheme:relaxation | family:dfs-flood |
    complexity:O(V) | source:https://leetcode.com/problems/pacific-atlantic-water-flow/

    Water flows from a cell to a not-higher orthogonal neighbour; a cell drains to an ocean iff water
    can reach that ocean's border. Reversing the flow, the cells that drain to an ocean are exactly
    those reachable from its border cells by climbing to not-lower neighbours. CLASSIFICATION: that
    drainage set is the reachable set from the ocean border `border` under the concrete reverse-flow
    relation `rflow height` (`q` is a not-lower orthogonal neighbour of `p`) — the least fixpoint of
    one-step relaxation. `cls` certifies the relaxation fixpoint; `corr` proves the fixpoint is exactly
    the cells reachable from the border by reverse flow — over a concrete relation, not a free `flow`.
    (The final answer intersects the two oceans' drainage sets, each computed as this lfp.) -/

namespace LC.P0417
open Interview

/-- A grid cell. -/
abbrev Cell := ℤ × ℤ

/-- Orthogonal (4-direction) adjacency on the integer grid. -/
def adjacent (p q : Cell) : Prop :=
  (p.1 = q.1 ∧ (q.2 = p.2 + 1 ∨ p.2 = q.2 + 1)) ∨ (p.2 = q.2 ∧ (q.1 = p.1 + 1 ∨ p.1 = q.1 + 1))

/-- Concrete reverse-flow step for grid heights: from `p` water climbs to an orthogonal neighbour `q`
    of not-lower height (so the ocean border reaches every cell that drains to it). -/
def rflow (height : Cell → ℤ) (p q : Cell) : Prop := adjacent p q ∧ height p ≤ height q

/-- The drainage set for one ocean: cells reachable from its `border` under reverse flow. -/
def sol (height : Cell → ℤ) (border : Set Cell) : Set Cell :=
  OrderHom.lfp (reachOp border (rflow height))

/-- Spec: exactly the cells reachable from the ocean `border` along reverse-flow steps. -/
def spec (height : Cell → ℤ) (border : Set Cell) (S : Set Cell) : Prop :=
  S = {v | ∃ u ∈ border, Relation.ReflTransGen (rflow height) u v}

/-- SCHEME (relaxation): the drainage set is a fixpoint of one-step relaxation of the reverse flow. -/
theorem cls (height : Cell → ℤ) (border : Set Cell) :
    reachOp border (rflow height) (sol height border) = sol height border :=
  reach_is_dp_fixpoint border (rflow height)

/-- CORRECT: the relaxation lfp is exactly the cells that drain to this ocean — those reachable from
    its border by reverse flow on the concrete grid heights. -/
theorem corr (height : Cell → ℤ) (border : Set Cell) : spec height border (sol height border) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]

end LC.P0417
