import Lproofs.Generators

/-! @lc 45 | name:Jump Game II | scheme:relaxation | family:bfs-greedy | complexity:O(n) |
    source:https://leetcode.com/problems/jump-game-ii/

    Minimum jumps to reach the last index, where from `i` you may step to any `j ≤ i + nums[i]`.
    CLASSIFICATION: the set of reachable indices from `0` is the least fixpoint of one-step relaxation
    under the jump relation `r` — the BFS layering the greedy farthest-reach scan compresses. `cls`
    certifies the relaxation fixpoint; `corr` ties it to genuine jump-reachability. DROP the minimum
    jump count (the BFS layer index). -/

namespace LC.P0045
open Interview

/-- Jump edge: forward, no further than `nums i`. -/
def jump (nums : ℕ → ℕ) (i j : ℕ) : Prop := i < j ∧ j ≤ i + nums i

/-- Reachable indices from the start, as the least fixpoint of one-step relaxation. -/
def sol (nums : ℕ → ℕ) : Set ℕ := OrderHom.lfp (reachOp {0} (jump nums))

/-- Spec: exactly the indices reachable from `0` along jump edges. -/
def spec (nums : ℕ → ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (jump nums) 0 v}

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls (nums : ℕ → ℕ) : reachOp {0} (jump nums) (sol nums) = sol nums :=
  reach_is_dp_fixpoint {0} (jump nums)

/-- CORRECT: the relaxation lfp is exactly the jump-reachable set from the start. -/
theorem corr (nums : ℕ → ℕ) : spec nums (sol nums) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0045
