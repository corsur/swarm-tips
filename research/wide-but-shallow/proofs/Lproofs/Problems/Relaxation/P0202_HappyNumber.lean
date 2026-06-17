import Lproofs.Generators

/-! @lc 202 | name:Happy Number | scheme:relaxation | family:dfs-flood | complexity:O(log n) |
    source:https://leetcode.com/problems/happy-number/

    Repeatedly replace `n` by the sum of the squares of its digits; `n` is happy iff this process
    reaches `1`. CLASSIFICATION: the set of values reachable from `n` under the step relation `r`
    (`r p q` ≜ `q` is the digit-square-sum of `p`) is the least fixpoint of one-step relaxation —
    `n` is happy iff `1` lies in that reachable set. (Floyd cycle detection is one evaluation of the
    same reachability question.) `cls` certifies the relaxation fixpoint; `corr` ties it to genuine
    reachability from `n`. DROP the happy/unhappy decision. -/

namespace LC.P0202
open Interview

/-- The values reachable from `n` under the digit-square-sum step `r`, as a relaxation lfp. -/
def sol (n : ℕ) (r : ℕ → ℕ → Prop) : Set ℕ := OrderHom.lfp (reachOp {n} r)

/-- Spec: exactly the values reachable from `n` along the step relation. -/
def spec (n : ℕ) (r : ℕ → ℕ → Prop) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen r n v}

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls (n : ℕ) (r : ℕ → ℕ → Prop) : reachOp {n} r (sol n r) = sol n r :=
  reach_is_dp_fixpoint {n} r

/-- CORRECT: the relaxation lfp is exactly the set of values reachable from `n` (happy iff `1` ∈ it). -/
theorem corr (n : ℕ) (r : ℕ → ℕ → Prop) : spec n r (sol n r) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0202
