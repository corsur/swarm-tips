import Lproofs.Schemes.Fold

/-! CITATION-BACKED (full correctness): the complete Tortoise-and-Hare algorithm — including
    Phase 2, locating the cycle's start — is machine-checked in the Archive of Formal Proofs
    ("The Tortoise and the Hare Algorithm", P. Gammie, 2015; `afp_tortoisehare` in the paper).
    The certificate below machine-checks the scheme membership and the Phase-1 meeting guarantee
    here; the remaining direction is cited, per the LC 1489/1192 precedent. -/
/-! @lc 142 | name:Linked List Cycle II | scheme:fold | family:fast-sol | complexity:O(n) |
    source:https://leetcode.com/problems/linked-list-cycle-ii/

    Floyd's two-pointer scan: a sol pointer advances one node per step, a fast pointer two; on a list
    with a cycle they are guaranteed to meet, which the second phase uses to locate the cycle entrance.
    CLASSIFICATION (fold): both pointers are streaming iterates of `next`. CORRECTNESS (the meeting that
    makes the algorithm work, not the entrance arithmetic): we certify that on an eventually-cyclic list
    the sol and fast pointers do meet---there is a positive step `t` at which `next^[t] = next^[2t]`. -/

namespace LC.P0142

/-- Slow pointer after `t` steps from the head: the `t`-th iterate of `next`. -/
def sol (next : ℕ → ℕ) (head : ℕ) (t : ℕ) : ℕ := next^[t] head

/-- SCHEME (fold): the sol pointer is a streaming iterate---one `next` step per tick. -/
theorem cls (next : ℕ → ℕ) (head t : ℕ) : sol next head (t + 1) = next (sol next head t) := by
  simp [sol, Function.iterate_succ_apply']

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

/-- CORRECT: on an eventually-cyclic list (cycle length `lam ≥ 1`), the fast and sol pointers meet ---
    there is a positive step `t` with `next^[t] head = next^[2t] head`. This is the Phase-1 guarantee
    Floyd's algorithm relies on. -/
theorem corr (next : ℕ → ℕ) (head mu lam : ℕ) (hlam : 1 ≤ lam)
    (hp : ∀ t, mu ≤ t → next^[t + lam] head = next^[t] head) :
    ∃ t, 0 < t ∧ sol next head t = sol next head (2 * t) := by
  simp only [sol]
  refine ⟨lam * (mu + 1), ?_, ?_⟩
  · exact Nat.mul_pos hlam (Nat.succ_pos mu)
  · have ht : mu ≤ lam * (mu + 1) := le_trans (Nat.le_succ mu) (Nat.le_mul_of_pos_left _ hlam)
    have hsplit : 2 * (lam * (mu + 1)) = lam * (mu + 1) + lam * (mu + 1) := by ring
    rw [hsplit, period_mul next head mu lam hp (mu + 1) (lam * (mu + 1)) ht]


/-- Official example 1 as a successor function: nodes 3→2→0→−4 with the tail joining the cycle
    at node index 1 (list indices 0..3; `next 3 = 1`). -/
def exNext : ℕ → ℕ
  | 0 => 1
  | 1 => 2
  | 2 => 3
  | 3 => 1
  | _ => 0

/-- GROUND INSTANCE (official example 1): the pointers meet — after 3 ticks slow sits where fast
    (2× speed) sits, exactly the Phase-1 meeting Floyd detects on this list. -/
theorem vec : sol exNext 0 3 = sol exNext 0 6 ∧ sol exNext 0 1 = 1 := by
  constructor <;> decide

end LC.P0142
