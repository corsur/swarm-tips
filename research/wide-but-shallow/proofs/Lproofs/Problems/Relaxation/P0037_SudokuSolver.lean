import Lproofs.Generators

/-! @lc 37 | name:Sudoku Solver | scheme:relaxation | family:backtracking-search | complexity:exp |
    source:https://leetcode.com/problems/sudoku-solver/

    Backtracking search fills the grid, placing a digit and recursing, undoing on conflict.
    CLASSIFICATION: the search explores exactly the set of partial-assignment states reachable from the
    initial board under the step relation `r` ("place one legal digit in the next empty cell") — the
    least fixpoint of one-step relaxation. `cls` certifies the relaxation fixpoint; `corr` ties it to
    genuine reachability of search states (a solution exists iff a complete state is reachable). DROP
    the solved-grid construction. -/

namespace LC.P0037
open Interview

/-- Reachable partial-assignment states from the initial board, as a relaxation lfp over `r`. -/
def sol {S : Type*} (start : S) (r : S → S → Prop) : Set S := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the search states reachable from the initial board along legal placements. -/
def spec {S : Type*} (start : S) (r : S → S → Prop) (T : Set S) : Prop :=
  T = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the reachable search states form a fixpoint of one-step relaxation. -/
theorem cls {S : Type*} (start : S) (r : S → S → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the reachable search states from the initial board. -/
theorem corr {S : Type*} (start : S) (r : S → S → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0037
