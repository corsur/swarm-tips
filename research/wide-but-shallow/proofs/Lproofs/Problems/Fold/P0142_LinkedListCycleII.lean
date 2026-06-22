import Lproofs.Schemes.Fold

/-! @lc 142 | name:Linked List Cycle II | scheme:fold | family:fast-slow | complexity:O(n) |
    source:https://leetcode.com/problems/linked-list-cycle-ii/

    Floyd's two-pointer scan: a slow pointer advances one node per step, a fast pointer two; on a list
    with a cycle they are guaranteed to meet, which the second phase uses to locate the cycle entrance.
    CLASSIFICATION (fold): both pointers are streaming iterates of `next`. CORRECTNESS (the meeting that
    makes the algorithm work, not the entrance arithmetic): we certify that on an eventually-cyclic list
    the slow and fast pointers do meet---there is a positive step `t` at which `next^[t] = next^[2t]`. -/

namespace LC.P0142

/-- Slow pointer after `t` steps from the head: the `t`-th iterate of `next`. -/
def slow (next : ℕ → ℕ) (head : ℕ) (t : ℕ) : ℕ := next^[t] head

/-- SCHEME (fold): the slow pointer is a streaming iterate---one `next` step per tick. -/
theorem cls (next : ℕ → ℕ) (head t : ℕ) : slow next head (t + 1) = next (slow next head t) := by
  simp [slow, Function.iterate_succ_apply']

/-- Eventual periodicity past the tail (length `mu`) with cycle length `lam`: applying `lam` more
    steps changes nothing once at least `mu` steps in. -/
theorem period_mul (next : ℕ → ℕ) (head mu lam : ℕ)
    (hp : ∀ t, mu ≤ t → next^[t + lam] head = next^[t] head) :
    ∀ (j t : ℕ), mu ≤ t → next^[t + lam * j] head = next^[t] head := by
  intro j
  induction j with
  | zero => intro t _; simp
  | succ j ih =>
    intro t ht
    have hstep : t + lam * (j + 1) = (t + lam * j) + lam := by ring
    rw [hstep, hp (t + lam * j) (le_trans ht (Nat.le_add_right _ _)), ih t ht]

/-- CORRECT: on an eventually-cyclic list (cycle length `lam ≥ 1`), the fast and slow pointers meet ---
    there is a positive step `t` with `next^[t] head = next^[2t] head`. This is the Phase-1 guarantee
    Floyd's algorithm relies on. -/
theorem corr (next : ℕ → ℕ) (head mu lam : ℕ) (hlam : 1 ≤ lam)
    (hp : ∀ t, mu ≤ t → next^[t + lam] head = next^[t] head) :
    ∃ t, 0 < t ∧ next^[t] head = next^[2 * t] head := by
  refine ⟨lam * (mu + 1), ?_, ?_⟩
  · exact Nat.mul_pos hlam (Nat.succ_pos mu)
  · have ht : mu ≤ lam * (mu + 1) := le_trans (Nat.le_succ mu) (Nat.le_mul_of_pos_left _ hlam)
    have hsplit : 2 * (lam * (mu + 1)) = lam * (mu + 1) + lam * (mu + 1) := by ring
    rw [hsplit, period_mul next head mu lam hp (mu + 1) (lam * (mu + 1)) ht]

end LC.P0142
