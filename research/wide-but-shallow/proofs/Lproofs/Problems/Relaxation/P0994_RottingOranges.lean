import Lproofs.Generators

/-! @lc 994 | name:Rotting Oranges | scheme:relaxation | family:bfs | complexity:O(mn) |
    source:https://leetcode.com/problems/rotting-oranges/

    Rot spreads from the initially-rotten oranges to adjacent fresh ones. CLASSIFICATION: the set of
    oranges that ever rot is exactly the cells reachable from the rotten sources under the fresh-orange
    adjacency relation — the least fixpoint of one-step relaxation (the BFS frontier). `cls` certifies
    it is a relaxation fixpoint; `corr` ties the fixpoint to genuine reachability. We certify the
    spreading structure, not the minute count. -/

namespace LC.P0994
open Interview

/-- Rot spreads to an adjacent fresh orange. -/
def spread (isFresh : ℕ → Prop) (adj : ℕ → ℕ → Prop) (p q : ℕ) : Prop := adj p q ∧ isFresh q

/-- The oranges that ever rot: cells reachable from the rotten `sources`, as the relaxation lfp. -/
def sol (isFresh : ℕ → Prop) (adj : ℕ → ℕ → Prop) (sources : Set ℕ) : Set ℕ :=
  OrderHom.lfp (reachOp sources (spread isFresh adj))

/-- Spec: exactly the cells reachable from a rotten source along fresh-orange adjacency. -/
def spec (isFresh : ℕ → Prop) (adj : ℕ → ℕ → Prop) (sources : Set ℕ) (S : Set ℕ) : Prop :=
  S = {v | ∃ u ∈ sources, Relation.ReflTransGen (spread isFresh adj) u v}

/-- SCHEME (relaxation): the rotted set is a fixpoint of one-step relaxation. -/
theorem cls (isFresh : ℕ → Prop) (adj : ℕ → ℕ → Prop) (sources : Set ℕ) :
    reachOp sources (spread isFresh adj) (sol isFresh adj sources) = sol isFresh adj sources :=
  reach_is_dp_fixpoint sources (spread isFresh adj)

/-- CORRECT: the relaxation lfp is exactly the set reachable from the rotten sources. -/
theorem corr (isFresh : ℕ → Prop) (adj : ℕ → ℕ → Prop) (sources : Set ℕ) :
    spec isFresh adj sources (sol isFresh adj sources) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]

end LC.P0994
