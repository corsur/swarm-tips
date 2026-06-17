import Lproofs.Generators

/-! @lc 130 | name:Surrounded Regions | scheme:relaxation | family:dfs-flood | complexity:O(mn) |
    source:https://leetcode.com/problems/surrounded-regions/

    Capture every `'O'` region not connected to the border. CLASSIFICATION: the SAFE cells (those that
    survive) are exactly the `'O'` cells reachable from the border set under same-symbol adjacency `r`
    — the least fixpoint of one-step relaxation from the multi-source border set. `cls` certifies the
    relaxation fixpoint; `corr` ties it to genuine border-reachability. DROP the capture/flip step. -/

namespace LC.P0130
open Interview

/-- The safe (un-captured) cells: reachable from the border set under same-symbol adjacency, as a
    relaxation lfp from the multi-source `border`. -/
def sol {V : Type*} (border : Set V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp border r)

/-- Spec: exactly the cells reachable from some border cell along `r`. -/
def spec {V : Type*} (border : Set V) (r : V → V → Prop) (S : Set V) : Prop :=
  S = {v | ∃ b ∈ border, Relation.ReflTransGen r b v}

/-- SCHEME (relaxation): the safe set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (border : Set V) (r : V → V → Prop) :
    reachOp border r (sol border r) = sol border r :=
  reach_is_dp_fixpoint border r

/-- CORRECT: the relaxation lfp is exactly the border-reachable cells (the surviving regions). -/
theorem corr {V : Type*} (border : Set V) (r : V → V → Prop) : spec border r (sol border r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]

end LC.P0130
