import Lproofs.Generators

/-! @lc 55 | name:Jump Game | scheme:relaxation | family:greedy | complexity:O(n) |
    source:https://leetcode.com/problems/jump-game/

    From index `i` you may step to any `j` with `i < j ≤ i + nums[i]`; you win if the last index is
    reachable from `0`. CLASSIFICATION: the set of reachable indices is exactly the reachable set from
    `{0}` under the jump relation — the least fixpoint of one-step relaxation. (The accepted greedy
    farthest-reach scan is one evaluation of that fixpoint.) `cls` certifies the relaxation fixpoint;
    `corr` ties it to genuine jump-reachability. -/

namespace LC.P0055
open Interview

/-- Jump edge: forward, no further than `nums i`. -/
def jump (nums : ℕ → ℕ) (i j : ℕ) : Prop := i < j ∧ j ≤ i + nums i

/-- The reachable indices from the start, as the least fixpoint of one-step relaxation. -/
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

end LC.P0055
