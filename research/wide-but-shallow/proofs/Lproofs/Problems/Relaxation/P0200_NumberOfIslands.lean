import Lproofs.Generators

/-! @lc 200 | name:Number of Islands | scheme:relaxation | family:dfs-flood | complexity:O(mn) |
    source:https://leetcode.com/problems/number-of-islands/

    Counting islands floods each one from a seed cell. CLASSIFICATION: an island is exactly the set of
    land cells reachable from a seed under the land-adjacency relation — the least fixpoint of one-step
    relaxation. `cls` certifies it is a relaxation fixpoint; `corr` ties the fixpoint to the genuine
    reachable land region (not an abstract placeholder). We certify the flood structure, not the count. -/

namespace LC.P0200
open Interview

/-- Step to an adjacent land cell (`isLand q` and `adj p q`). -/
def flood (isLand : ℕ → Prop) (adj : ℕ → ℕ → Prop) (p q : ℕ) : Prop := adj p q ∧ isLand q

/-- The island of `seed`: the land cells reachable from it, as the least fixpoint of relaxation. -/
def sol (isLand : ℕ → Prop) (adj : ℕ → ℕ → Prop) (seed : ℕ) : Set ℕ :=
  OrderHom.lfp (reachOp {seed} (flood isLand adj))

/-- Spec: exactly the cells reachable from `seed` along land-adjacency. -/
def spec (isLand : ℕ → Prop) (adj : ℕ → ℕ → Prop) (seed : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (flood isLand adj) seed v}

/-- SCHEME (relaxation): the island is a fixpoint of one-step relaxation. -/
theorem cls (isLand : ℕ → Prop) (adj : ℕ → ℕ → Prop) (seed : ℕ) :
    reachOp {seed} (flood isLand adj) (sol isLand adj seed) = sol isLand adj seed :=
  reach_is_dp_fixpoint {seed} (flood isLand adj)

/-- CORRECT: the relaxation lfp is exactly the flood-reachable island. -/
theorem corr (isLand : ℕ → Prop) (adj : ℕ → ℕ → Prop) (seed : ℕ) :
    spec isLand adj seed (sol isLand adj seed) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0200
