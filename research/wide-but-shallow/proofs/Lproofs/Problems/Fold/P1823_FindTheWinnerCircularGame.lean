import Lproofs.Schemes.Fold

/-! @lc 1823 | name:Find the Winner of the Circular Game | scheme:fold | family:math-bit |
    complexity:O(n) | source:https://leetcode.com/problems/find-the-winner-of-the-circular-game/

    The survivor of the Josephus elimination (every `k`-th person removed from a circle) is computed by
    the recurrence `J(1)=0`, `J(m+1) = (J(m) + k) mod (m+1)`. CLASSIFICATION (fold): the survivor is a
    left fold of this recurrence over the growing circle size. CORRECTNESS: we certify that the computed
    survivor is always a valid seat index (`J(n) < n` for `n ≥ 1`) — the modulus keeps it in range — so
    the one-pass recurrence genuinely names a person in the circle. -/

namespace LC.P1823
open Interview.Patterns

/-- 0-indexed Josephus survivor among `m` people eliminating every `k`-th. -/
def joseph (k : ℕ) : ℕ → ℕ
  | 0 => 0
  | n + 1 => (joseph k n + k) % (n + 1)

/-- SCHEME (fold): the survivor is a left fold of the recurrence over circle sizes `1..n`. -/
theorem cls (k n : ℕ) :
    joseph k n = (List.range n).foldl (fun acc i => (acc + k) % (i + 1)) 0 := by
  induction n with
  | zero => rfl
  | succ m ih => rw [joseph, ih, List.range_succ, List.foldl_append]; rfl

/-- CORRECT: the survivor is always a valid seat — `J(n+1) < n+1` — since the modulus bounds it. -/
theorem corr (k n : ℕ) : joseph k (n + 1) < n + 1 := by
  simp only [joseph]
  exact Nat.mod_lt _ (Nat.succ_pos n)

end LC.P1823
