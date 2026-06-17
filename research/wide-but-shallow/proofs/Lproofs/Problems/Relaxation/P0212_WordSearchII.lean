import Lproofs.Generators

/-! @lc 212 | name:Word Search II | scheme:relaxation | family:dfs-flood | complexity:O(m·n·4^L) |
    source:https://leetcode.com/problems/word-search-ii/

    Trie-guided DFS over a grid finding every dictionary word. CLASSIFICATION: the search frontier is
    exactly the set of (cell, trie-node) states reachable from a start state under the step relation `r`
    ("move to an adjacent cell whose letter continues a trie edge") — the least fixpoint of one-step
    relaxation. `cls` certifies the relaxation fixpoint; `corr` ties it to genuine reachability of
    search states (a word is found iff an accepting state is reachable). DROP the found-word collection. -/

namespace LC.P0212
open Interview

/-- Reachable (cell, trie-node) search states from `start`, as a relaxation lfp over `r`. -/
def sol {S : Type*} (start : S) (r : S → S → Prop) : Set S := OrderHom.lfp (reachOp {start} r)

/-- Spec: exactly the search states reachable from `start` along the trie-guided step relation. -/
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

end LC.P0212
