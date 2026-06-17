import Lproofs.Generators

/-! @lc 1871 | name:Jump Game VII | scheme:relaxation | family:reachability |
    complexity:O(n) | source:https://leetcode.com/problems/jump-game-vii/

    From index `i` you may jump to `j` when `minJ ≤ j − i ≤ maxJ` and cell `j` is open. You win if the
    last index is reachable from `0`. CLASSIFICATION: the set of reachable indices is exactly the
    reachable set from `{0}` under the jump relation — the least fixpoint of one-step relaxation. The
    accepted prefix-sum DP is just an efficient evaluation of that fixpoint. `cls` certifies it is a
    relaxation fixpoint; `corr` ties the fixpoint to genuine jump-reachability (not a placeholder). -/

namespace LC.P1871
open Interview

/-- Jump edge: forward within the window `[minJ, maxJ]` to an open cell (`isOpen j`). -/
def jump (minJ maxJ : ℕ) (isOpen : ℕ → Prop) (i j : ℕ) : Prop :=
  i < j ∧ minJ ≤ j - i ∧ j - i ≤ maxJ ∧ isOpen j

/-- The reachable indices from the start, as the least fixpoint of one-step relaxation. -/
def sol (minJ maxJ : ℕ) (isOpen : ℕ → Prop) : Set ℕ :=
  OrderHom.lfp (reachOp {0} (jump minJ maxJ isOpen))

/-- Spec: exactly the indices reachable from `0` along jump edges. -/
def spec (minJ maxJ : ℕ) (isOpen : ℕ → Prop) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (jump minJ maxJ isOpen) 0 v}

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls (minJ maxJ : ℕ) (isOpen : ℕ → Prop) :
    reachOp {0} (jump minJ maxJ isOpen) (sol minJ maxJ isOpen) = sol minJ maxJ isOpen :=
  reach_is_dp_fixpoint {0} (jump minJ maxJ isOpen)

/-- CORRECT: the relaxation lfp is exactly the jump-reachable set from the start. -/
theorem corr (minJ maxJ : ℕ) (isOpen : ℕ → Prop) : spec minJ maxJ isOpen (sol minJ maxJ isOpen) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P1871
