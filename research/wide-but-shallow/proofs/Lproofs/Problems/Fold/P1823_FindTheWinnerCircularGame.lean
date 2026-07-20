import Lproofs.Schemes.Fold

/-! TIER NOTE (2026-07-19): the modelled `sol` IS the accepted solution verbatim — the textbook
    Josephus recurrence J(1)=0, J(n+1)=(J(n)+k) mod (n+1) — with zero modelling distance, and
    `cls` proves the EXACT identity that it is a left fold over circle sizes. `corr` (the answer
    is always a valid seat) is an exact invariant of that definitional algorithm. A separate
    elimination-process semantics is not modelled — the platform's acceptance is the correctness
    oracle for the recurrence solving the game (paper §2/§4). -/
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
def sol (k : ℕ) : ℕ → ℕ
  | 0 => 0
  | n + 1 => (sol k n + k) % (n + 1)

/-- SCHEME (fold): the survivor is a left fold of the recurrence over circle sizes `1..n`. -/
theorem cls (k n : ℕ) :
    sol k n = (List.range n).foldl (fun acc i => (acc + k) % (i + 1)) 0 := by
  induction n with
  | zero => rfl
  | succ m ih => rw [sol, ih, List.range_succ, List.foldl_append]; rfl

/-- CORRECT: the survivor is always a valid seat — `J(n+1) < n+1` — since the modulus bounds it. -/
theorem corr (k n : ℕ) : sol k (n + 1) < n + 1 := by
  simp only [sol]
  exact Nat.mod_lt _ (Nat.succ_pos n)


/-- GROUND INSTANCE (official example 1): n = 5, k = 2 — the winner is friend 3, i.e. 0-indexed
    seat 2. -/
theorem vec : sol 2 5 = 2 := by decide

end LC.P1823
