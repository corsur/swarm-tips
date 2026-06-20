import Lproofs.Generators

/-! @lc 212 | name:Word Search II | scheme:relaxation | family:dfs-flood | complexity:O(m·n·4^L) |
    source:https://leetcode.com/problems/word-search-ii/

    Trie-guided DFS over a grid finding every dictionary word. A search state is a (cell, trie-node)
    pair, and the next state moves to an adjacent cell whose letter continues a trie edge.
    CLASSIFICATION: the search frontier is exactly the set of states reachable from a start state under
    the concrete step relation `step g` (`g` is the search-state adjacency encoding the trie-guided
    moves) — the least fixpoint of one-step relaxation. `cls` certifies the relaxation fixpoint; `corr`
    proves the fixpoint is exactly the reachable search states (a word is found iff an accepting state
    is reachable) — over a concrete relation, not a free `r`. -/

namespace LC.P0212
open Interview

/-- Concrete trie-guided step: `t` is a successor search-state of `s` (adjacency `g` encodes
    "move to an adjacent grid cell whose letter continues a trie edge"). -/
def step (g : ℕ → List ℕ) (s t : ℕ) : Prop := t ∈ g s

/-- Reachable search states from `start`, as the least relaxation fixpoint. -/
def sol (g : ℕ → List ℕ) (start : ℕ) : Set ℕ := OrderHom.lfp (reachOp {start} (step g))

/-- Spec: exactly the search states reachable from `start` along trie-guided steps. -/
def spec (g : ℕ → List ℕ) (start : ℕ) (T : Set ℕ) : Prop :=
  T = {v | Relation.ReflTransGen (step g) start v}

/-- SCHEME (relaxation): the reachable search states form a fixpoint of one-step relaxation. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    reachOp {start} (step g) (sol g start) = sol g start :=
  reach_is_dp_fixpoint {start} (step g)

/-- CORRECT: the relaxation lfp is exactly the search states reachable from `start` along the
    trie-guided moves (so a word is found iff its accepting state is reachable). -/
theorem corr (g : ℕ → List ℕ) (start : ℕ) : spec g start (sol g start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0212
