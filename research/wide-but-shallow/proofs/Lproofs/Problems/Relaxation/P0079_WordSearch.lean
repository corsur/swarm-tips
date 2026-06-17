import Lproofs.Generators

/-! @lc 79 | name:Word Search | scheme:relaxation | family:dfs-flood | complexity:O(m·n·4^L) |
    source:https://leetcode.com/problems/word-search/

    DFS from each grid cell, walking to adjacent cells that continue the target word. CLASSIFICATION:
    the search frontier is exactly the set of (cell, word-position) states reachable from a start state
    under the step relation `r` ("move to an adjacent cell whose letter is the next character") — the
    least fixpoint of one-step relaxation. `cls` certifies the relaxation fixpoint; `corr` ties it to
    genuine reachability of search states (the word is found iff an accepting state is reachable).
    DROP the found/not-found decision. -/

namespace LC.P0079
open Interview

/-- Reachable DFS search states from a start state, as a relaxation lfp over the step relation `r`. -/
def sol {S : Type*} (start : S) (r : S → S → Prop) : Set S := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the states reachable from `start` along the search-step relation. -/
def spec {S : Type*} (start : S) (r : S → S → Prop) (T : Set S) : Prop :=
  T = {v | Relation.ReflTransGen r start v}

/-- SCHEME (relaxation): the reachable search states form a fixpoint of one-step relaxation. -/
theorem cls {S : Type*} (start : S) (r : S → S → Prop) :
    reachOp {start} r (sol start r) = sol start r :=
  reach_is_dp_fixpoint {start} r

/-- CORRECT: the relaxation lfp is exactly the reachable search states from `start`. -/
theorem corr {S : Type*} (start : S) (r : S → S → Prop) : spec start r (sol start r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0079
